import { IconButton } from "@aipanel/ui";

const Copy = () => (
  <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.4}>
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
    <path d="M3.5 10.5H3a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1h6.5a1 1 0 0 1 1 1v.5" />
  </svg>
);
const Plus = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round">
    <line x1="8" y1="3.5" x2="8" y2="12.5" />
    <line x1="3.5" y1="8" x2="12.5" y2="8" />
  </svg>
);

export function Variants() {
  return (
    <div className="flex items-center gap-3">
      <IconButton aria-label="复制">
        <Copy />
      </IconButton>
      <IconButton aria-label="添加" variant="bordered">
        <Plus />
      </IconButton>
    </div>
  );
}

export function Sizes() {
  return (
    <div className="flex items-center gap-3">
      <IconButton aria-label="复制" size="sm">
        <Copy />
      </IconButton>
      <IconButton aria-label="复制" size="md">
        <Copy />
      </IconButton>
      <IconButton aria-label="复制" size="lg">
        <Copy />
      </IconButton>
    </div>
  );
}
