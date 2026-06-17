import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@aipanel/ui";

export function ServerSummary() {
  return (
    <Card className="max-w-sm">
      <CardHeader>
        <div>
          <CardTitle>web-prod-1</CardTitle>
          <CardDescription>root@10.0.0.4:22</CardDescription>
        </div>
        <Badge tone="success">online</Badge>
      </CardHeader>
      <CardContent>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
          <div className="flex justify-between">
            <dt className="text-fg-subtle">OS</dt>
            <dd className="text-fg">Ubuntu 22.04</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-fg-subtle">CPU</dt>
            <dd className="text-fg">12%</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-fg-subtle">Mem</dt>
            <dd className="text-fg">3.1/8 GB</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-fg-subtle">Disk</dt>
            <dd className="text-fg">44%</dd>
          </div>
        </dl>
      </CardContent>
      <CardFooter>
        <Button size="sm" variant="secondary">
          Doctor
        </Button>
        <Button size="sm">Ask</Button>
      </CardFooter>
    </Card>
  );
}

export function Minimal() {
  return (
    <Card className="max-w-sm">
      <CardHeader>
        <CardTitle>Audit trail</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-fg-muted">
          No commands have been run on this server yet.
        </p>
      </CardContent>
    </Card>
  );
}
