import type { ErrorKind } from "@/bindings/ErrorKind";
import { ja } from "@/i18n/ja";

interface ErrorScreenProps {
  kind: ErrorKind;
  message: string;
}

/** アプリを開始できなかったとき（DB を開けないなど）に、全画面で原因を見せる。 */
export function ErrorScreen({ kind, message }: ErrorScreenProps) {
  return (
    <main
      role="alert"
      className="mx-auto flex min-h-screen max-w-xl flex-col justify-center gap-4 p-8 text-slate-800"
    >
      <h1 className="text-2xl font-semibold text-red-700">{ja.errorScreen.title}</h1>
      <p>{ja.errorScreen.hints[kind]}</p>
      <section className="rounded-lg border border-red-200 bg-red-50 p-4">
        <h2 className="mb-1 text-sm font-medium text-red-700">{ja.errorScreen.reason}</h2>
        <p className="text-sm break-words">{message}</p>
      </section>
    </main>
  );
}
