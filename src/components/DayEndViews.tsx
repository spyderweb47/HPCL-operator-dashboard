import { Bike, Cylinder } from "lucide-react";
import type { CylinderStockDTO, DeliveryDTO } from "@/lib/types";

export const STOCK_ROWS = [
  { key: "kg5", label: "5 kg" },
  { key: "kg14", label: "14.2 kg" },
  { key: "kg19", label: "19 kg" },
] as const;

export function Card({ title, icon: Icon, children, aside }: { title: string; icon: typeof Bike; children: React.ReactNode; aside?: React.ReactNode }) {
  return (
    <section className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3">
        <h2 className="flex items-center gap-2 font-semibold text-slate-800">
          <Icon className="h-4 w-4 text-hp-blue" /> {title}
        </h2>
        {aside}
      </div>
      {children}
    </section>
  );
}

export function DeliveriesView({ deliveries }: { deliveries: DeliveryDTO[] }) {
  const bills = deliveries.reduce((s, d) => s + d.billsPrinted, 0);
  const cyl = deliveries.reduce((s, d) => s + d.cylindersDelivered, 0);
  return (
    <Card title="Delivery Boy Logs" icon={Bike}>
      <table className="w-full text-sm">
        <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-2">Delivery Boy</th>
            <th className="px-4 py-2 text-right">Bills Printed</th>
            <th className="px-4 py-2 text-right">Cylinders Delivered</th>
            <th className="px-4 py-2 text-right">Difference</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {deliveries.length === 0 && (
            <tr>
              <td colSpan={4} className="px-4 py-8 text-center text-slate-400">
                No delivery logs recorded.
              </td>
            </tr>
          )}
          {deliveries.map((d) => {
            const diff = d.billsPrinted - d.cylindersDelivered;
            return (
              <tr key={d.deliveryBoyId}>
                <td className="px-4 py-2 font-medium">{d.deliveryBoyName}</td>
                <td className="tabular px-4 py-2 text-right">{d.billsPrinted}</td>
                <td className="tabular px-4 py-2 text-right">{d.cylindersDelivered}</td>
                <td className={`tabular px-4 py-2 text-right font-semibold ${diff === 0 ? "text-slate-400" : "text-amber-700"}`}>
                  {diff > 0 ? `+${diff}` : diff}
                </td>
              </tr>
            );
          })}
        </tbody>
        {deliveries.length > 0 && (
          <tfoot className="border-t-2 border-slate-200 bg-slate-50 font-semibold">
            <tr>
              <td className="px-4 py-2">Total</td>
              <td className="tabular px-4 py-2 text-right">{bills}</td>
              <td className="tabular px-4 py-2 text-right">{cyl}</td>
              <td className="tabular px-4 py-2 text-right">{bills - cyl > 0 ? `+${bills - cyl}` : bills - cyl}</td>
            </tr>
          </tfoot>
        )}
      </table>
    </Card>
  );
}

export function StockView({ stock }: { stock: CylinderStockDTO }) {
  return (
    <Card title="Closing Cylinder Stock" icon={Cylinder}>
      <table className="w-full text-sm">
        <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-2">Cylinder</th>
            <th className="px-4 py-2 text-right">Full</th>
            <th className="px-4 py-2 text-right">Empty</th>
            <th className="px-4 py-2 text-right">Total</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {STOCK_ROWS.map(({ key, label }) => (
            <tr key={key}>
              <td className="px-4 py-2 font-medium">{label}</td>
              <td className="tabular px-4 py-2 text-right">{stock[key].full}</td>
              <td className="tabular px-4 py-2 text-right">{stock[key].empty}</td>
              <td className="tabular px-4 py-2 text-right font-semibold">{stock[key].full + stock[key].empty}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}
