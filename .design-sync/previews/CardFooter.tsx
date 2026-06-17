import { Button, Card, CardContent, CardFooter, CardHeader, CardTitle } from "@aipanel/ui";

export function InCard() {
  return (
    <Card className="max-w-sm">
      <CardHeader>
        <CardTitle>Restart nginx</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-fg-muted">
          This is a medium-risk action. Confirm to proceed.
        </p>
      </CardContent>
      <CardFooter>
        <Button size="sm" variant="ghost">
          Discard
        </Button>
        <Button size="sm" variant="danger">
          Approve &amp; run
        </Button>
      </CardFooter>
    </Card>
  );
}
