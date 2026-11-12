import { motion, useAnimation } from "motion/react";
import type { HTMLAttributes } from "react";
import { forwardRef, useImperativeHandle } from "react";

import { cn } from "@/lib/utils";

export interface SquareIconHandle {
  startAnimation: () => void;
  stopAnimation: () => void;
}

interface SquareIconProps extends HTMLAttributes<HTMLDivElement> {
  size?: number;
}

/** "Stop". Not part of the upstream animated set, built to the same contract. */
const SquareIcon = forwardRef<SquareIconHandle, SquareIconProps>(
  ({ className, size = 28, ...props }, ref) => {
    const controls = useAnimation();

    useImperativeHandle(ref, () => ({
      startAnimation: () => controls.start("animate"),
      stopAnimation: () => controls.start("normal"),
    }));

    return (
      <div className={cn(className)} {...props}>
        <svg
          fill="none"
          height={size}
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="2"
          viewBox="0 0 24 24"
          width={size}
          xmlns="http://www.w3.org/2000/svg"
        >
          <motion.rect
            animate={controls}
            height="14"
            initial="normal"
            rx="2"
            style={{ transformOrigin: "12px 12px" }}
            variants={{
              normal: { scale: 1, rotate: 0 },
              animate: {
                scale: [1, 0.78, 1],
                rotate: [0, 90, 90],
                transition: { duration: 0.45, ease: "easeInOut" },
              },
            }}
            width="14"
            x="5"
            y="5"
          />
        </svg>
      </div>
    );
  },
);

SquareIcon.displayName = "SquareIcon";

export { SquareIcon };
