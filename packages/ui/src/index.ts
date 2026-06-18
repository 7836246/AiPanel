// 工具函数
export { cn } from "./lib/cn";

// 基础组件（primitives）
export { Button, type ButtonProps } from "./components/primitives/Button";
export { Badge, type BadgeProps } from "./components/primitives/Badge";
export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
} from "./components/primitives/Card";
export { IconButton, type IconButtonProps } from "./components/primitives/IconButton";
export { Input, type InputProps } from "./components/primitives/Input";
export { Switch, type SwitchProps } from "./components/primitives/Switch";
export { Select, type SelectProps, type SelectOption } from "./components/primitives/Select";
export { Textarea, type TextareaProps } from "./components/primitives/Textarea";
export { Spinner, type SpinnerProps } from "./components/primitives/Spinner";
export { CodeBlock, type CodeBlockProps } from "./components/primitives/CodeBlock";
export {
  Terminal,
  type TerminalProps,
  type TerminalLine,
  type TerminalLineTone,
} from "./components/primitives/Terminal";
export { Dialog, type DialogProps } from "./components/primitives/Dialog";
export {
  useToasts,
  ToastViewport,
  type Toast,
} from "./components/primitives/Toast";

// 领域组件（domain）
export { RiskBadge, type RiskBadgeProps } from "./components/domain/RiskBadge";
export { RISK_META, type RiskLevel } from "./components/domain/risk";
export {
  ServerCard,
  type ServerCardProps,
  type ServerStatus,
} from "./components/domain/ServerCard";
export {
  CommandPlan,
  type CommandPlanProps,
  type PlanStep,
} from "./components/domain/CommandPlan";
export { AuditEntry, type AuditEntryProps } from "./components/domain/AuditEntry";
