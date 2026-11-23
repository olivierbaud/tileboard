// Turning values into text. Pure functions: no page, no network.

/** Units that go in front of the number: "$", "NZ$", "€", "£". */
const CURRENCY = /^[A-Z]{0,3}[$€£¥₹₩₽₺₪₫฿]$/u;

/** Units that touch the number: "45%", "21°C". Every other unit gets a thin space: "48.9 GB". */
const TIGHT = /^(?:[%‰′″]|°.*)$/u;

const THIN_SPACE = " ";

/** Shown where there is no data right now. */
export const NO_DATA = "–";

export const STATUS_WORDS = { ok: "OK", warn: "Warning", crit: "Critical" };

export function isCurrency(unit) {
  return typeof unit === "string" && CURRENCY.test(unit);
}

/**
 * A number split for display: { prefix, number, suffix }.
 * The dashboard does not invent precision: digits come out as the app sent them,
 * except money, which always shows cents unless it is a whole amount.
 * `options.compact` shortens big numbers (12.9K, 4.2M). `options.digits` fixes the
 * number of decimals (for a column, see `columnDigits`). `options.locale` is for tests.
 */
export function numberParts(value, unit, options = {}) {
  const { locale, compact = false, digits } = options;
  const currency = isCurrency(unit);
  const abs = Math.abs(value);
  let number;
  let negative = value < 0;

  if (compact && abs >= 100_000) {
    number = new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 }).format(abs);
  } else if (digits !== undefined) {
    number = new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(abs);
    negative = negative && Number(abs.toFixed(digits)) !== 0;
  } else if (currency) {
    const cents = Math.round(abs * 100);
    const digits = cents % 100 === 0 ? 0 : 2;
    number = new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(
      cents / 100,
    );
    negative = negative && cents !== 0;
  } else {
    negative = negative && abs >= 0.0005;
    number = new Intl.NumberFormat(locale, { maximumFractionDigits: 3 }).format(abs);
  }

  const sign = negative ? "-" : "";
  if (currency) return { prefix: sign + unit, number, suffix: "" };
  const suffix = !unit ? "" : TIGHT.test(unit) ? unit : THIN_SPACE + unit;
  return { prefix: sign, number, suffix };
}

export function formatNumber(value, unit, options) {
  const { prefix, number, suffix } = numberParts(value, unit, options);
  return prefix + number + suffix;
}

/**
 * The decimals to give every number of a column, so the column lines up:
 * money shows cents as soon as one amount has them; other numbers take the
 * most decimals any of them has (3 at most).
 */
export function columnDigits(values, unit) {
  const decimals = (value) => Math.min(3, (String(Number(value.toFixed(3))).split(".")[1] ?? "").length);
  const most = Math.max(0, ...values.filter((value) => typeof value === "number").map(decimals));
  return isCurrency(unit) && most > 0 ? 2 : most;
}

/** How long ago, in the biggest unit that fits: "45 s", "3 min", "2 h", "4 d". */
export function formatAge(seconds) {
  const s = Math.max(0, Math.round(seconds));
  if (s < 90) return `${s} s`;
  const minutes = Math.round(s / 60);
  if (minutes < 90) return `${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 36) return `${hours} h`;
  return `${Math.round(hours / 24)} d`;
}

/** One field's value as a short text, for rows and labels. `spec` is the field from the catalogue. */
export function entryText(spec, entry, options) {
  const value = entry?.value;
  if (value === null || value === undefined) return NO_DATA;
  switch (spec.type) {
    case "number":
    case "series":
      return formatNumber(value, spec.unit, options);
    case "status":
      return STATUS_WORDS[value] ?? String(value);
    case "list":
      return value.length === 1 ? "1 item" : `${value.length} items`;
    default:
      return String(value);
  }
}

/** The value of one list item as text. A numeric value takes the field's unit. */
export function itemText(item, unit, options) {
  if (typeof item.value === "number") return formatNumber(item.value, unit, options);
  return item.value ?? "";
}
