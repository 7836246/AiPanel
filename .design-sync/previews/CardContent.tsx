import { Card, CardContent, CardHeader, CardTitle } from "@aipanel/ui";

export function InCard() {
  return (
    <Card className="max-w-sm">
      <CardHeader>
        <CardTitle>Diagnosis</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-fg-muted">
          nginx is installed but the service is inactive. Restarting it should
          recover the site.
        </p>
      </CardContent>
    </Card>
  );
}
