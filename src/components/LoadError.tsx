import { TriangleAlert } from "lucide-react";

export function LoadError({ message }: { message: string }) {
  return (
    <div role="alert" className="flex items-center gap-3 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-red-800">
      <TriangleAlert className="h-5 w-5" />
      <p className="font-medium">{message}</p>
    </div>
  );
}
