import { Card, CardHeader, CardTitle, CardDescription } from "@aipanel/ui";

export function InCard() {
  return (
    <Card className="max-w-sm">
      <CardHeader>
        <div>
          <CardTitle>Execution plan</CardTitle>
          <CardDescription>
            Recover the unreachable website on web-prod-1
          </CardDescription>
        </div>
      </CardHeader>
    </Card>
  );
}
