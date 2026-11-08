import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sidebarMotion, SIDEBAR_LAYOUT_MS } from './sidebar-motion.ts';

function fixture(t: { after(fn: () => void): void }, reduced = false) {
  function global(name: string, value: unknown) {
    const original = Object.getOwnPropertyDescriptor(globalThis, name);
    Object.defineProperty(globalThis, name, { configurable: true, value });
    t.after(() => original ? Object.defineProperty(globalThis, name, original) : Reflect.deleteProperty(globalThis, name));
  }
  global('matchMedia', () => ({ matches: reduced }));
  global('getComputedStyle', () => ({ color: 'white', font: '14px sans-serif', opacity: '1' }));
  let origin = { left: 10, top: 20 };
  let position = { left: 30, top: 100, width: 16, height: 16 };
  let flightPosition = position;
  const calls: { frames: Keyframe[]; options: KeyframeAnimationOptions; animation: Animation }[] = [];
  const cancelled: Animation[] = [];
  const ghosts: { removed: boolean }[] = [];
  const layer = { style: { cssText: '' }, setAttribute() {}, append() {}, remove() {} };
  global('document', { createElement: () => layer });
  const icon = {
    dataset: { sidebarIcon: 'tab:one' }, style: { opacity: '' },
    closest: () => null,
    getBoundingClientRect: () => position,
    cloneNode() {
      const ghost = {
        style: { cssText: '' }, removed: false,
        removeAttribute() {},
        remove() { ghost.removed = true; },
        getBoundingClientRect: () => flightPosition,
        animate(frames: Keyframe[], options: KeyframeAnimationOptions) {
          const animation = { cancel() { cancelled.push(animation); }, onfinish: null } as unknown as Animation;
          calls.push({ frames, options, animation });
          return animation;
        },
      };
      ghosts.push(ghost);
      return ghost;
    },
  };
  let nodes = [icon];
  const root = {
    getBoundingClientRect: () => origin, querySelectorAll: () => nodes, append() {},
  } as unknown as HTMLElement;
  return {
    motion: sidebarMotion(root), calls, cancelled, icon, ghosts, layer,
    move(left: number, top: number) { position = { left, top, width: 16, height: 16 }; },
    moveFlight(left: number, top: number) { flightPosition = { left, top, width: 16, height: 16 }; },
    moveRoot(left: number, top: number) { origin = { left, top }; },
    replace() { nodes = [{ ...icon }]; },
    remove() { nodes = []; },
  };
}

test('Alt+B moves unclipped icon copies without scaling their glyphs', (t) => {
  const f = fixture(t);
  const before = f.motion.capture();
  f.move(40, 60);
  f.motion.play(before);
  assert.deepEqual(f.calls[0].frames, [{ translate: '-10px 40px' }, { translate: '0px 0px' }]);
  assert.equal(f.calls[0].options.duration, SIDEBAR_LAYOUT_MS);
  assert.equal(f.icon.style.opacity, '0');
  assert.match(f.layer.style.cssText, /overflow:visible/);
  f.calls[0].animation.onfinish?.call(f.calls[0].animation, {} as AnimationPlaybackEvent);
  assert.equal(f.icon.style.opacity, '');
  assert.equal(f.ghosts[0].removed, true);
});

test('rapid reversal captures the flying copy before cancelling old motion', (t) => {
  const f = fixture(t);
  let before = f.motion.capture();
  f.move(40, 60);
  f.motion.play(before);
  f.moveFlight(35, 80);
  before = f.motion.capture();
  assert.deepEqual(f.cancelled, [f.calls[0].animation]);
  assert.equal(f.ghosts[0].removed, true);
  f.move(30, 100);
  f.motion.play(before);
  assert.deepEqual(f.calls[1].frames[0], { translate: '5px -20px' });
});

test('relative coordinates avoid animating the sidebar top edge twice', (t) => {
  const f = fixture(t);
  const before = f.motion.capture();
  f.moveRoot(10, 60);
  f.move(30, 140);
  f.motion.play(before);
  assert.equal(f.calls.length, 0);
});

test('stable keys connect replaced icons and ignore removed icons', (t) => {
  const f = fixture(t);
  const before = f.motion.capture();
  f.replace();
  f.move(40, 60);
  f.motion.play(before);
  assert.equal(f.calls.length, 1);
  f.remove();
  assert.doesNotThrow(() => f.motion.capture());
});

test('reduced motion skips position animation', (t) => {
  const f = fixture(t, true);
  const before = f.motion.capture();
  f.move(40, 60);
  f.motion.play(before);
  assert.equal(f.calls.length, 0);
});

test('destroy cancels motion, removes copies and restores original icons', (t) => {
  const f = fixture(t);
  const before = f.motion.capture();
  f.move(40, 60);
  f.motion.play(before);
  f.motion.destroy();
  assert.deepEqual(f.cancelled, [f.calls[0].animation]);
  assert.equal(f.icon.style.opacity, '');
  assert.equal(f.ghosts[0].removed, true);
});
