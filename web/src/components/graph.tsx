import { SAMPLES } from "@/lib/history";
import { cn } from "@/lib/utils";

/**
 * The last three minutes as one column per sample, newest on the right.
 * Columns the panel has not measured yet stay as a line on the floor.
 */
export function Bars({
  values,
  max,
  label,
  className,
}: {
  values: number[];
  max: number;
  label: string;
  className?: string;
}) {
  const offset = SAMPLES - values.length;
  const top = max > 0 ? max : 1;
  return (
    <svg
      viewBox={`0 0 ${SAMPLES} 100`}
      preserveAspectRatio="none"
      shapeRendering="crispEdges"
      role="img"
      aria-label={label}
      className={cn("h-16 w-full text-primary", className)}
    >
      {Array.from({ length: SAMPLES }, (_, slot) => {
        const value = slot >= offset ? values[slot - offset]! : undefined;
        const height = value === undefined ? 0 : Math.min(100, Math.max(0, (value / top) * 100));
        return (
          <g key={slot}>
            <rect x={slot + 0.15} y={97} width={0.7} height={3} className="fill-muted" />
            {height > 0 && (
              <rect
                x={slot + 0.15}
                y={100 - Math.max(3, height)}
                width={0.7}
                height={Math.max(3, height)}
                fill="currentColor"
              />
            )}
          </g>
        );
      })}
    </svg>
  );
}

/** A share of a whole, as a bar that fills from the left. */
export function Meter({ value, label }: { value: number; label: string }) {
  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value)}
      className="h-2.5 bg-muted"
    >
      <div
        className="h-full bg-primary transition-[width] duration-500"
        style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
      />
    </div>
  );
}
