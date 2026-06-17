import { Badge, Card, CardHeader, CardTitle, CardDescription } from "@aipanel/ui";

export function InCard() {
  return (
    <Card className="max-w-sm">
      <CardHeader>
        <div>
          <CardTitle>web-prod-1</CardTitle>
          <CardDescription>root@10.0.0.4:22</CardDescription>
        </div>
        <Badge tone="success">online</Badge>
      </CardHeader>
    </Card>
  );
}
