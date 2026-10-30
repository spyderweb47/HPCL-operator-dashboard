export function PageSkeleton() {
  return (
    <div className="space-y-4">
      <div className="h-24 animate-pulse rounded-xl bg-slate-200" />
      <div className="h-20 animate-pulse rounded-xl bg-slate-200" />
      <div className="h-80 animate-pulse rounded-xl bg-slate-200" />
    </div>
  );
}
