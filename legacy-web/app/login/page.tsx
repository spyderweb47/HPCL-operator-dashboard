import { Fuel } from "lucide-react";
import { LoginForm } from "./LoginForm";

export default function LoginPage() {
  return (
    <main className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex items-center justify-center gap-3">
          <div className="flex h-11 w-11 items-center justify-center rounded-lg bg-hp-blue text-white">
            <Fuel className="h-6 w-6" />
          </div>
          <div>
            <p className="text-lg font-bold leading-tight text-hp-blue">HPCL Dealership</p>
            <p className="text-sm text-slate-600">Office Operator Portal</p>
          </div>
        </div>
        <div className="rounded-xl border border-slate-200 bg-white p-6 shadow-sm">
          <LoginForm />
        </div>
      </div>
    </main>
  );
}
