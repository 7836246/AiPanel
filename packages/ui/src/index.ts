// Utilities
export { cn } from "./lib/cn";

// Primitives
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

// Domain
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
