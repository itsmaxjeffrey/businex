import { describe, expect, it } from "vitest";
import { formatMoney, formatDate, timeAgo } from "./ui";

describe("formatting helpers", () => {
  it("formats money with currency", () => {
    expect(formatMoney(1100, "USD")).toContain("1,100");
    expect(formatMoney(0, "EUR")).toContain("0");
  });

  it("formats dates and never crashes on empty values", () => {
    expect(formatDate(null)).toBe("—");
    expect(formatDate(undefined)).toBe("—");
    expect(formatDate("2026-10-07T00:00:00.000Z")).toMatch(/2026/);
  });

  it("describes relative time", () => {
    const now = Date.now();
    expect(timeAgo(new Date(now - 30 * 1000).toISOString())).toBe("just now");
    expect(timeAgo(new Date(now - 5 * 60 * 1000).toISOString())).toBe("5m ago");
    expect(timeAgo(new Date(now - 3 * 3600 * 1000).toISOString())).toBe("3h ago");
  });
});
