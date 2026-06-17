import { forwardRef } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/cn";

const iconButton = cva(
  "inline-flex shrink-0 items-center justify-center rounded-md text-fg-muted transition-colors hover:bg-hover hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        ghost: "border-none bg-transparent",
        bordered: "border border-border-strong bg-surface-1",
      },
      size: {
        sm: "h-6 w-6",
        md: "h-7 w-7",
        lg: "h-8 w-8",
      },
    },
    defaultVariants: { variant: "ghost", size: "md" },
  }
);

export interface IconButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof iconButton> {
  /** 无障碍名称——纯图标按钮没有可见文字，必须提供。 */
  "aria-label": string;
}

/** 方形纯图标按钮。用于工具栏/窗口/行内操作（复制、切换等）。 */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ className, variant, size, type = "button", ...props }, ref) => (
    <button
      ref={ref}
      type={type}
      className={cn(iconButton({ variant, size }), className)}
      {...props}
    />
  )
);
IconButton.displayName = "IconButton";
