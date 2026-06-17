# AiPanel gpt-image-2 Asset Prompts

These prompts are prepared for the image generation CLI:

```sh
export IMAGE_GEN="$HOME/.codex/skills/.system/imagegen/scripts/image_gen.py"
python "$IMAGE_GEN" generate \
  --model gpt-image-2 \
  --quality high \
  --size 1024x1024 \
  --prompt-file assets/prompts/logo.prompt.txt \
  --out output/imagegen/aipanel-logo-gpt-image-2.png
```

The current environment needs `OPENAI_API_KEY` before real generation can run.

## App icon prompt

Use case: logo-brand
Asset type: app icon
Primary request: Create a modern logo for "AiPanel", a local AI server operations client that connects to Linux servers over SSH.
Subject: A distinctive app icon combining a terminal prompt, AI node graph, and lightweight server panel concept.
Style/medium: polished vector-like 3D app icon, clean SaaS/open-source branding, high contrast, production quality.
Composition/framing: centered square icon, readable at small sizes, generous padding, no tiny details.
Color palette: deep graphite, emerald green, teal, cyan highlights, white foreground accents.
Materials/textures: smooth glassy surfaces, soft depth, subtle shadow, no noisy texture.
Text: no text inside the image.
Constraints: no copyrighted logos, no resemblance to 1Panel, BaoTa, Docker, GitHub, OpenAI, Apple, or existing brand marks; no Chinese characters; no watermark; no mockup device frame.
Avoid: busy dashboard screenshots, generic robot head, shield-only icon, cloud-only icon, gradients that overpower the mark.

## Horizontal logo prompt

Use case: logo-brand
Asset type: README brand logo
Primary request: Create a horizontal brand logo for "AiPanel", a Chinese open-source local AI server operations client.
Subject: A distinctive app icon on the left, with the brand text "AiPanel" and the Chinese tagline "本地 AI 运维客户端" on the right.
Style/medium: polished vector-like brand lockup, clean open-source project identity, high contrast, production quality.
Composition/framing: wide horizontal logo, transparent-looking clean background, icon and wordmark aligned, readable in a GitHub README header.
Color palette: deep graphite, emerald green, teal, cyan highlights, white and slate text.
Text (verbatim): "AiPanel" and "本地 AI 运维客户端".
Constraints: Chinese text must be clear and correctly written; no copyrighted logos; no resemblance to 1Panel, BaoTa, Docker, GitHub, OpenAI, Apple, or existing brand marks; no watermark.
Avoid: distorted text, extra slogans, busy dashboard screenshots, generic robot head, shield-only icon, cloud-only icon.

## Preview prompt

Use case: ui-mockup
Asset type: README hero preview image
Primary request: Create a polished Chinese product preview image for AiPanel, a local AI server operations desktop client.
Scene/backdrop: clean desktop software interface on a light background.
Subject: The UI shows a server list on the left, an AI task plan in the center, and SSH execution output on the right.
Style/medium: high-fidelity product UI mockup, modern open-source project README hero image.
Composition/framing: 16:9 landscape, centered interface, readable panels, no excessive decoration.
Lighting/mood: crisp, trustworthy, technical, lightweight.
Color palette: white, slate, emerald green, teal, cyan accents.
Text: use short Chinese interface labels only: "服务器", "AI 任务计划", "SSH 执行", "风险：低", "确认执行", "只读检查", "端口与服务", "日志诊断".
Constraints: Chinese text must be clear and correctly written; no real company logos; no brand names other than AiPanel; no screenshots of existing products; no tiny unreadable paragraphs.
Avoid: cyberpunk style, dark-only UI, stock server room photos, 3D robots, cluttered dashboards.
