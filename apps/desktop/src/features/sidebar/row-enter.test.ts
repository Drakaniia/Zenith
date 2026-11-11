import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { rowEnter, ROW_ENTER_GUARD_MS } from './row-enter.ts';

function fixture() {
  const animation = { cancelled: 0, onfinish: null as null | (() => void), cancel() { this.cancelled++; } };
  const node = { animate: mock.fn(() => animation) } as unknown as HTMLElement;
  return { node, animation, animate: (node as unknown as { animate: ReturnType<typeof mock.fn> }).animate };
}

test('rows of the first render do not animate', () => {
  const f = fixture();
  assert.equal(rowEnter(f.node, false), undefined);
  assert.equal(f.animate.mock.callCount(), 0);
});

test('a stalled animation is cancelled so the row cannot stay hidden', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  t.mock.method(console, 'warn', () => {});
  const f = fixture();
  rowEnter(f.node, true);
  assert.equal(f.animate.mock.callCount(), 1);
  t.mock.timers.tick(ROW_ENTER_GUARD_MS);
  assert.equal(f.animation.cancelled, 1);
});

test('a finished animation disarms the guard', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = fixture();
  rowEnter(f.node, true);
  f.animation.onfinish?.();
  t.mock.timers.tick(ROW_ENTER_GUARD_MS * 2);
  assert.equal(f.animation.cancelled, 0);
});

test('removing the row cancels its animation', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = fixture();
  rowEnter(f.node, true)?.destroy();
  assert.equal(f.animation.cancelled, 1);
});
