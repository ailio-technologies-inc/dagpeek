import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

// An original pixel pup for Dagpeek, with the app's green collar. Each row is 16 pixels.
const COLOURS: Record<string, string> = { b: '#d9a066', l: '#f0d2a4', d: '#7a4a2a', k: '#1b1e25', c: '#12a06e' };
const FRAMES = {
  "wag1": [
    "...........dd...",
    "..........dbbbb.",
    "..........bkbbbk",
    "..........blllll",
    "..........ccll..",
    ".d......bbbbb...",
    ".dbbbbbbbbbbb...",
    "..bbbbbbbbbbl...",
    "..bbbbbbbbbll...",
    "..bb.b....b.bb..",
    "..d..d....d.d..."
  ],
  "wag2": [
    "...........dd...",
    "..........dbbbb.",
    "..........bkbbbk",
    "..........blllll",
    "..........ccll..",
    "........bbbbb...",
    "ddbbbbbbbbbbb...",
    "..bbbbbbbbbbl...",
    "..bbbbbbbbbll...",
    "..bb.b....b.bb..",
    "..d..d....d.d..."
  ],
  "blink": [
    "...........dd...",
    "..........dbbbb.",
    "..........bdbbbk",
    "..........blllll",
    "..........ccll..",
    "........bbbbb...",
    "ddbbbbbbbbbbb...",
    "..bbbbbbbbbbl...",
    "..bbbbbbbbbll...",
    "..bb.b....b.bb..",
    "..d..d....d.d..."
  ],
  "run1": [
    "...........dd...",
    "..........dbbbb.",
    "..........bkbbbk",
    "..........blllll",
    "..........ccll..",
    ".d......bbbbb...",
    ".dbbbbbbbbbbb...",
    "..bbbbbbbbbbl...",
    "..bbbbbbbbbll...",
    ".b.......bb.b...",
    ".d.......d...d.."
  ],
  "run2": [
    "...........dd...",
    "..........dbbbb.",
    "..........bkbbbk",
    "..........blllll",
    "..........ccll..",
    "d.......bbbbb...",
    "ddbbbbbbbbbbb...",
    "..bbbbbbbbbbl...",
    "..bbbbbbbbbll...",
    "...bb..b.b......",
    "...d...d..d....."
  ],
  "sleep": [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..........dd....",
    "...bbbbbbbbdbbb.",
    "..bbbbbbbbbbdbbk",
    "ddbbbbbbbbbbllll",
    "..bbbbbbbbbccll.",
    "..dd.......dd...",
    "................"
  ]
} as const;
type Frame = keyof typeof FRAMES;

function Sprite({ frame }: { frame: Frame }) {
  const rects = useMemo(
    () => FRAMES[frame].flatMap((row, y) => [...row].flatMap((ch, x) => (COLOURS[ch] ? [<rect key={`${x}-${y}`} x={x} y={y} width="1" height="1" fill={COLOURS[ch]} />] : []))),
    [frame],
  );
  return <svg className="dog-sprite" viewBox="0 0 16 13" shapeRendering="crispEdges" aria-hidden="true">{rects}</svg>;
}

const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches || 'dagpeekStill' in window;

/** Runs in the first time the popover opens after launch, then wags, or sleeps when nothing's on. */
export function Dog({ asleep }: { asleep: boolean }) {
  const [tick, setTick] = useState(0);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    if (still()) return;
    let seen = false;
    const off = listen('popover-opened', () => {
      if (seen) return;
      seen = true;
      setRunning(true);
      setTimeout(() => setRunning(false), 1200);
    });
    const timer = setInterval(() => setTick((t) => t + 1), 150);
    return () => {
      clearInterval(timer);
      void off.then((f) => f());
    };
  }, []);

  let frame: Frame;
  if (running) frame = tick % 2 ? 'run1' : 'run2';
  else if (asleep) frame = 'sleep';
  else if (tick % 24 === 23) frame = 'blink';
  else frame = Math.floor(tick / 2) % 2 ? 'wag1' : 'wag2';

  return <span className={running ? 'dog dog-run' : 'dog'}><Sprite frame={frame} /></span>;
}
