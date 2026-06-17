import { cn } from "../../lib/cn";
import { Badge } from "../primitives/Badge";
import { Card, CardContent, CardHeader, CardTitle } from "../primitives/Card";

export type ServerStatus = "online" | "offline" | "unknown";

export interface ServerCardProps extends React.HTMLAttributes<HTMLDivElement> {
  name: string;
  /** 连接目标，例如 "root@10.0.0.4:22"。记录前需脱敏。 */
  host: string;
  status?: ServerStatus;
  /** 以键值网格展示的简要信息，例如 { OS: "Ubuntu 22.04", CPU: "12%" }。 */
  facts?: Record<string, string>;
}

const STATUS_TONE = {
  online: "success",
  offline: "danger",
  unknown: "neutral",
} as const;

/** 展示一台已保存服务器：连接目标、可达状态与简要信息。 */
export function ServerCard({
  name,
  host,
  status = "unknown",
  facts,
  className,
  ...props
}: ServerCardProps) {
  return (
    <Card className={cn("transition-colors hover:border-border-strong", className)} {...props}>
      <CardHeader>
        <div className="min-w-0">
          <CardTitle className="truncate">{name}</CardTitle>
          <p className="mt-0.5 truncate font-mono text-xs text-fg-subtle">{host}</p>
        </div>
        <Badge tone={STATUS_TONE[status]} className="capitalize">
          {status}
        </Badge>
      </CardHeader>
      {facts && Object.keys(facts).length > 0 ? (
        <CardContent>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5">
            {Object.entries(facts).map(([key, value]) => (
              <div key={key} className="flex items-baseline justify-between gap-2">
                <dt className="text-xs text-fg-subtle">{key}</dt>
                <dd className="truncate text-xs font-medium text-fg">{value}</dd>
              </div>
            ))}
          </dl>
        </CardContent>
      ) : null}
    </Card>
  );
}
