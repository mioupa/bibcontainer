//! quick-xml で読んだ XML を小さな木にする。
//!
//! NDL の応答（RDF/XML）は名前空間つきで、1 つの項目が入れ子になることが多い
//! （`dc:title/rdf:Description/rdf:value`）。イベントを追いながら処理するより、
//! 木にして名前空間 URI + ローカル名で引く方が読みやすいので、必要な範囲だけの木を作る。

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::{NsReader, XmlVersion};

/// 入れ子の深さの上限（悪意のある応答への備え）。
const MAX_DEPTH: usize = 64;
/// 要素数の上限。NDL の実際の応答は 1 レコードあたり数百要素。
const MAX_NODES: usize = 400_000;

#[derive(Debug, Clone, Default)]
pub struct Attr {
    pub ns: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    /// 名前空間 URI（なければ空）
    pub ns: String,
    /// ローカル名（接頭辞なし）
    pub name: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    /// 直下のテキスト（連結したもの。前後の空白はそのまま）
    pub text: String,
}

impl Node {
    pub fn is(&self, ns: &str, name: &str) -> bool {
        self.name == name && self.ns == ns
    }

    pub fn child(&self, ns: &str, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.is(ns, name))
    }

    pub fn children_named<'a>(
        &'a self,
        ns: &'a str,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.is(ns, name))
    }

    pub fn attr(&self, ns: &str, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name == name && a.ns == ns)
            .map(|a| a.value.as_str())
    }

    /// 前後の空白を除いたテキスト。
    pub fn text(&self) -> &str {
        self.text.trim()
    }

    /// 自分と子孫をすべて（深さ優先）。
    pub fn descendants(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            out.push(n);
            for c in n.children.iter().rev() {
                stack.push(c);
            }
        }
        out
    }
}

fn ns_string(r: &ResolveResult<'_>) -> String {
    match r {
        ResolveResult::Bound(ns) => ns.0.to_string(),
        _ => String::new(),
    }
}

fn build_node(e: &BytesStart<'_>, ns: String, reader: &NsReader<&[u8]>) -> Result<Node, String> {
    let mut node = Node {
        ns,
        name: e.local_name().into_inner().to_string(),
        ..Node::default()
    };
    for attr in e.attributes() {
        let attr = attr.map_err(|err| format!("属性を読めません: {err}"))?;
        if attr.key.0 == "xmlns" || attr.key.0.starts_with("xmlns:") {
            continue;
        }
        let (attr_ns, local) = reader.resolver().resolve_attribute(attr.key);
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|err| format!("属性の値を読めません: {err}"))?
            .into_owned();
        node.attrs.push(Attr {
            ns: ns_string(&attr_ns),
            name: local.into_inner().to_string(),
            value,
        });
    }
    Ok(node)
}

fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => {
            if root.is_none() {
                *root = Some(node);
            }
        }
    }
}

/// XML 全体を読んで、ルート要素を返す。
pub fn parse_document(xml: &str) -> Result<Node, String> {
    let mut reader = NsReader::from_str(xml);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut count = 0usize;

    loop {
        let (resolved, event) = reader
            .read_resolved_event()
            .map_err(|e| format!("XML を読めません: {e}"))?;
        let ns = ns_string(&resolved);
        match event {
            Event::Start(e) => {
                count += 1;
                if count > MAX_NODES {
                    return Err("XML の要素数が多すぎます".to_string());
                }
                let node = build_node(&e, ns, &reader)?;
                stack.push(node);
                if stack.len() > MAX_DEPTH {
                    return Err("XML の入れ子が深すぎます".to_string());
                }
            }
            Event::Empty(e) => {
                count += 1;
                if count > MAX_NODES {
                    return Err("XML の要素数が多すぎます".to_string());
                }
                let node = build_node(&e, ns, &reader)?;
                attach(&mut stack, &mut root, node);
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| "対応する開始タグのない終了タグがあります".to_string())?;
                attach(&mut stack, &mut root, node);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&c.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    match r.resolve_char_ref() {
                        Ok(Some(ch)) => top.text.push(ch),
                        Ok(None) => match r.xml10_content().as_ref() {
                            "amp" => top.text.push('&'),
                            "lt" => top.text.push('<'),
                            "gt" => top.text.push('>'),
                            "quot" => top.text.push('"'),
                            "apos" => top.text.push('\''),
                            // 定義されていない実体は展開しない（DTD で定義された実体による膨張を防ぐ）
                            _ => {}
                        },
                        Err(e) => return Err(format!("文字参照を読めません: {e}")),
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("閉じられていない要素があります".to_string());
    }
    root.ok_or_else(|| "XML に要素がありません".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tree_with_namespaces_and_entities() {
        let xml = r#"<?xml version="1.0"?>
<a:root xmlns:a="urn:a" xmlns:b="urn:b" b:attr="1 &amp; 2">
  <b:item>こんにちは &lt;世界&gt; &#x41;</b:item>
  <b:item/>
  <item>plain</item>
  <![CDATA[<raw>]]>
</a:root>"#;
        let root = parse_document(xml).unwrap();
        assert!(root.is("urn:a", "root"));
        assert_eq!(root.attr("urn:b", "attr"), Some("1 & 2"));
        let items: Vec<_> = root.children_named("urn:b", "item").collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text(), "こんにちは <世界> A");
        assert!(items[1].children.is_empty());
        assert_eq!(root.child("", "item").map(|n| n.text()), Some("plain"));
        assert!(root.text.contains("<raw>"));
        assert_eq!(root.descendants().len(), 4);
    }

    #[test]
    fn rejects_broken_xml() {
        assert!(parse_document("<a><b></a>").is_err());
        assert!(parse_document("<a>").is_err());
        assert!(parse_document("").is_err());
    }

    #[test]
    fn unknown_entities_are_dropped_not_expanded() {
        let xml = r#"<!DOCTYPE a [<!ENTITY big "AAAAAAAAAA">]><a>x&big;y</a>"#;
        let root = parse_document(xml).unwrap();
        assert_eq!(root.text(), "xy");
    }
}
