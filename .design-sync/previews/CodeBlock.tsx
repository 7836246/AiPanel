import { CodeBlock } from "@aipanel/ui";

export function Command() {
  return (
    <div className="max-w-lg">
      <CodeBlock label="$ command">systemctl restart nginx</CodeBlock>
    </div>
  );
}

export function Output() {
  return (
    <div className="max-w-lg">
      <CodeBlock label="stdout">
        {"● nginx.service - A high performance web server\n   Active: inactive (dead)\n   Docs: https://nginx.org/en/docs/"}
      </CodeBlock>
    </div>
  );
}

export function Bare() {
  return (
    <div className="max-w-lg">
      <CodeBlock>journalctl -u nginx -n 50 --no-pager</CodeBlock>
    </div>
  );
}
