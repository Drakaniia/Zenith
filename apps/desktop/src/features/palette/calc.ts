// The palette's calculator: a small recursive-descent parser for + - * / % ^ and parentheses.
// Anything that isn't purely arithmetic with at least one binary operator yields null.

/** The value of `input` as arithmetic, or null when it isn't math (or divides by zero). */
export function calc(input: string): number | null {
  const s = input.replace(/\s+/g, "");
  if (!s) return null;
  let i = 0;
  let ops = 0;

  // expr := term (('+' | '-') term)*
  function expr(): number {
    let v = term();
    while (s[i] === "+" || s[i] === "-") {
      const op = s[i++];
      ops++;
      const r = term();
      v = op === "+" ? v + r : v - r;
    }
    return v;
  }
  // term := unary (('*' | '/' | '%') unary)*
  function term(): number {
    let v = unary();
    while (s[i] === "*" || s[i] === "/" || s[i] === "%") {
      const op = s[i++];
      ops++;
      const r = unary();
      if (op !== "*" && r === 0) throw new Error("division by zero");
      v = op === "*" ? v * r : op === "/" ? v / r : v % r;
    }
    return v;
  }
  // unary := ('-' | '+') unary | power   (so -2^2 is -(2^2), as in maths)
  function unary(): number {
    if (s[i] === "-") return i++, -unary();
    if (s[i] === "+") return i++, unary();
    return power();
  }
  // power := primary ('^' unary)?   (right-associative)
  function power(): number {
    const base = primary();
    if (s[i] !== "^") return base;
    i++;
    ops++;
    return base ** unary();
  }
  // primary := number | '(' expr ')'
  function primary(): number {
    if (s[i] === "(") {
      i++;
      const v = expr();
      if (s[i++] !== ")") throw new Error("missing )");
      return v;
    }
    const m = /^(\d+\.?\d*|\.\d+)/.exec(s.slice(i));
    if (!m) throw new Error("expected a number");
    i += m[0].length;
    return Number(m[0]);
  }

  try {
    const v = expr();
    if (i !== s.length || ops === 0 || !Number.isFinite(v)) return null;
    // Trim float noise like 0.1 + 0.2 = 0.30000000000000004.
    return Number(v.toPrecision(12));
  } catch {
    return null;
  }
}
