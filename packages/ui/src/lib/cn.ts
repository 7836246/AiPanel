import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** 合并条件类名，并消解冲突的 Tailwind 工具类。 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
