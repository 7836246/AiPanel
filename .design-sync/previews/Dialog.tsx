import { Button, Dialog } from "@aipanel/ui";

export function ConfirmRestart() {
  return (
    <Dialog
      open
      onClose={() => {}}
      title="Confirm medium-risk action"
      description="This will restart nginx on web-prod-1. The service will be briefly unavailable."
      footer={
        <>
          <Button variant="secondary" size="sm">
            Cancel
          </Button>
          <Button variant="danger" size="sm">
            Confirm restart
          </Button>
        </>
      }
    >
      Review the plan once more before approving. High-risk steps would require a
      second confirmation.
    </Dialog>
  );
}
