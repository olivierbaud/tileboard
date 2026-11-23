// Run with: node --test "tests/web/*.test.js"   (`cargo test` does it too, when Node is installed)

import assert from "node:assert/strict";
import test from "node:test";

import {
  NO_DATA,
  columnDigits,
  entryText,
  formatAge,
  formatNumber,
  isCurrency,
  itemText,
  numberParts,
} from "../../web/format.js";

const US = { locale: "en-US" };
const THIN = " ";

test("money goes before the number and shows cents unless it is a whole amount", () => {
  assert.equal(formatNumber(437.02, "$", US), "$437.02");
  assert.equal(formatNumber(80.5, "$", US), "$80.50");
  assert.equal(formatNumber(600, "$", US), "$600");
  assert.equal(formatNumber(1234.5, "NZ$", US), "NZ$1,234.50");
  assert.equal(formatNumber(5, "€", US), "€5");
  assert.equal(formatNumber(-12.5, "$", US), "-$12.50");
  assert.equal(formatNumber(-0.001, "$", US), "$0");
});

test("which units count as money", () => {
  for (const unit of ["$", "€", "£", "¥", "NZ$", "A$", "US$"]) assert.ok(isCurrency(unit), unit);
  for (const unit of ["%", "GB", "km/h", "°C", "", undefined, "dollars"]) assert.ok(!isCurrency(unit), String(unit));
});

test("percent and degrees touch the number; other units get a thin space", () => {
  assert.equal(formatNumber(43.9, "%", US), "43.9%");
  assert.equal(formatNumber(9.4, "°C", US), "9.4°C");
  assert.equal(formatNumber(48.9, "GB", US), `48.9${THIN}GB`);
  assert.equal(formatNumber(60.2, "km/h", US), `60.2${THIN}km/h`);
  assert.equal(formatNumber(7, undefined, US), "7");
  assert.equal(formatNumber(7, "", US), "7");
});

test("digits come out as the app sent them", () => {
  assert.equal(formatNumber(2.5, undefined, US), "2.5");
  assert.equal(formatNumber(1234567, undefined, US), "1,234,567");
  assert.equal(formatNumber(0.1 + 0.2, undefined, US), "0.3");
  assert.equal(formatNumber(-3.25, "%", US), "-3.25%");
  assert.equal(formatNumber(-0.0001, "%", US), "0%");
  assert.equal(formatNumber(0, "GB", US), `0${THIN}GB`);
});

test("big numbers can be shortened", () => {
  const compact = { ...US, compact: true };
  assert.equal(formatNumber(12_900, undefined, compact), "12,900");
  assert.equal(formatNumber(1_234_567, undefined, compact), "1.2M");
  assert.equal(formatNumber(4_200_000, "$", compact), "$4.2M");
  assert.equal(formatNumber(-250_000, "", compact), "-250K");
  assert.equal(formatNumber(-250_000, "$", compact), "-$250K");
});

test("a number comes in parts, so the unit can be set smaller", () => {
  assert.deepEqual(numberParts(43.9, "%", US), { prefix: "", number: "43.9", suffix: "%" });
  assert.deepEqual(numberParts(437.02, "$", US), { prefix: "$", number: "437.02", suffix: "" });
  assert.deepEqual(numberParts(-48.9, "GB", US), { prefix: "-", number: "48.9", suffix: `${THIN}GB` });
});

test("ages use the biggest unit that fits", () => {
  const cases = [
    [0, "0 s"],
    [-5, "0 s"],
    [45, "45 s"],
    [89, "89 s"],
    [90, "2 min"],
    [150, "3 min"],
    [3600, "60 min"],
    [5400, "2 h"],
    [7200, "2 h"],
    [36 * 3600, "2 d"],
    [10 * 86400, "10 d"],
  ];
  for (const [seconds, expected] of cases) assert.equal(formatAge(seconds), expected, String(seconds));
});

test("a field's value as text", () => {
  const options = US;
  assert.equal(entryText({ type: "number", unit: "%" }, { value: 43.9 }, options), "43.9%");
  assert.equal(entryText({ type: "series", unit: "$" }, { value: 18.55, series: [1, 18.55] }, options), "$18.55");
  assert.equal(entryText({ type: "text" }, { value: "12d 4h" }, options), "12d 4h");
  assert.equal(entryText({ type: "status" }, { value: "ok" }, options), "OK");
  assert.equal(entryText({ type: "status" }, { value: "warn" }, options), "Warning");
  assert.equal(entryText({ type: "status" }, { value: "crit" }, options), "Critical");
  assert.equal(entryText({ type: "list" }, { value: [{ label: "a" }] }, options), "1 item");
  assert.equal(entryText({ type: "list" }, { value: [{ label: "a" }, { label: "b" }] }, options), "2 items");
  assert.equal(entryText({ type: "list" }, { value: [] }, options), "0 items");
});

test("no data is a dash, whatever the type", () => {
  for (const type of ["number", "series", "text", "status", "list"]) {
    assert.equal(entryText({ type }, { value: null }), NO_DATA);
    assert.equal(entryText({ type }, undefined), NO_DATA);
  }
});

test("a column of numbers gets one number of decimals", () => {
  // Money: cents for all as soon as one amount has them.
  assert.equal(columnDigits([190, 142.72, 46.7], "$"), 2);
  assert.equal(columnDigits([190, 140], "$"), 0);
  // Other numbers: the most decimals any of them has, 3 at most.
  assert.equal(columnDigits([14, 15.5, 13], "°C"), 1);
  assert.equal(columnDigits([1, 2.25, 3.5], undefined), 2);
  assert.equal(columnDigits([0.12345678], "GB"), 3);
  assert.equal(columnDigits([0.1 + 0.2], undefined), 1, "float noise is not precision");
  // Texts and missing values do not count.
  assert.equal(columnDigits(["restarting", undefined, 4], undefined), 0);
  assert.equal(columnDigits([], "$"), 0);

  assert.equal(formatNumber(190, "$", { ...US, digits: 2 }), "$190.00");
  assert.equal(formatNumber(14, "°C", { ...US, digits: 1 }), "14.0°C");
  assert.equal(formatNumber(-0.04, "%", { ...US, digits: 1 }), "0.0%");
  assert.equal(itemText({ label: "Groceries", value: 190 }, "$", { ...US, digits: 2 }), "$190.00");
});

test("a list item's value", () => {
  assert.equal(itemText({ label: "Groceries", value: 212.77 }, "$", US), "$212.77");
  assert.equal(itemText({ label: "Mon", value: 14 }, "°C", US), "14°C");
  assert.equal(itemText({ label: "mail", value: "restarting" }, undefined, US), "restarting");
  assert.equal(itemText({ label: "web" }, undefined, US), "");
});
