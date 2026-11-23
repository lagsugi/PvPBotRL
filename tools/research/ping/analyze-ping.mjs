// Run with Node.js after the ping_sweep Rust example. No external packages.
import fs from 'node:fs/promises';
const root = new URL('../../../', import.meta.url);
const out = new URL('build/benchmarks/ping-sweep/', root);
const data = JSON.parse(await fs.readFile(new URL('results.json', out), 'utf8'));
const pings = Array.from({length:15}, (_, i) => (i + 1) * 10);
const styles = ['critical-plus', 'duel-rl'];
const name = s => s === 'critical-plus' ? 'Critical+' : 'Duel-RL';
const n = g => g.episodes.length;
const pct = g => 100 * g.wins / n(g);
const split = g => Math.round((g.condition.up_ms[0] + g.condition.down_ms[0]) * 1000) / 1000;
const grid = (s, a, b) => data.find(g => g.condition.style === s && g.condition.family === 'rtt-grid' && g.condition.up_ms[0]*2 === Math.min(a,b) && g.condition.up_ms[1]*2 === Math.max(a,b));
const rate = (s,a,b) => { const g=grid(s,a,b); return 100*(a<=b?g.wins:g.losses)/n(g); };
let state = 20261106;
const rand = () => { state = (Math.imul(state,1664525)+1013904223) >>> 0; return state/4294967296; };
function interval(g) {
  const clusters = Array.from({length:64},(_,v)=>g.episodes.filter(e=>e.variant===v).filter(e=>e.winner_role===0).length/4);
  const samples = Array.from({length:5000},()=>100*clusters.reduce(sum=>sum+clusters[Math.floor(rand()*64)],0)/64).sort((a,b)=>a-b);
  return [samples[125],samples[4874]].map(x=>x.toFixed(1)).join('–');
}
// Assert counts, symmetric controls, and identical outcomes with diagnostic logging enabled.
for(const g of data) {
  if(n(g)!==256||g.wins+g.losses+g.timeouts!==256) throw Error('invalid counts');
  if(g.condition.family==='rtt-grid'&&g.condition.up_ms[0]===g.condition.up_ms[1]&&g.wins!==128) throw Error('asymmetric control');
  if(g.condition.diagnostics) {
    const k=g.condition;
    const other=data.find(h=>!h.condition.diagnostics&&h.condition.style===k.style&&['up_ms','down_ms'].every(f=>h.condition[f].every((v,i)=>Math.abs(v-k[f][i])<1e-7)));
    if(!other||JSON.stringify(g.episodes.map(e=>[e.winner_role,e.hp,e.seconds]))!==JSON.stringify(other.episodes.map(e=>[e.winner_role,e.hp,e.seconds]))) throw Error('trace altered outcomes');
  }
}
const csv=['style,family,role0_up_ms,role0_down_ms,role1_up_ms,role1_down_ms,wins,losses,timeouts,win_pct,mean_hp_difference,mean_seconds',
  ...data.map(g=>[g.condition.style,g.condition.family,g.condition.up_ms[0],g.condition.down_ms[0],g.condition.up_ms[1],g.condition.down_ms[1],g.wins,g.losses,g.timeouts,pct(g),g.mean_hp_difference,g.mean_seconds].join(','))].join('\n');
await fs.writeFile(new URL('summary.csv',out),csv+'\n');
let report=`# Measured effect of ping on match outcomes (November 6, 2026)

Critical+ mirror matches and bundled Duel-RL mirror matches totaled **112,128 games**, with zero timeouts. Ping means **RTT = upstream + downstream delay**. Baseline comparisons split RTT equally between the two directions.

The lower-ping player won **67.56%** of Critical+ matches and **74.27%** of Duel-RL matches on average. Each of the 105 unequal-ping pairs contributed 256 matches with equal weight. Both policies reached at least 50% on the lower-ping side in every pair, but win rate varied nonmonotonically with the gap. For example, Critical+ won 78.13% at 10 vs 100 ms and 74.22% at 10 vs 150 ms. These results apply to the tested simulator policies and conditions. Live Minecraft remains unverified.

## Representative matchups

Win rates are for the lower-ping player. Each row contains 256 matches per policy. The 95% intervals use 5,000 cluster bootstrap samples over 64 conditions. The four seat and processing-order variants within a condition belong to the same cluster.

|RTT (low vs high)|Critical+ wins–losses|Win rate / 95% interval|Duel-RL wins–losses|Win rate / 95% interval|
|---|---:|---:|---:|---:|
`;
for(const [a,b] of [[10,50],[10,100],[10,150],[50,100],[50,150],[100,150]]) {
  const x=grid(styles[0],a,b),y=grid(styles[1],a,b);
  report+=`|${a} vs ${b} ms|${x.wins}–${x.losses}|${pct(x).toFixed(2)}% / ${interval(x)}%|${y.wins}–${y.losses}|${pct(y).toFixed(2)}% / ${interval(y)}%|
`;
}
report+=`
## Upstream/downstream split at fixed RTT

At each RTT, vary the tested player's upstream share across 10, 25, 50, 75, and 90%, keeping the opponent at the same RTT with a 50:50 split. Total RTT stays constant. The results for all RTTs are in summary.csv.

At 100 ms, an additional sweep varies upstream delay from 5 to 95 ms in 5 ms increments. The opponent always has 50 ms each way. Reducing upstream delay increases downstream delay, so this comparison cannot isolate their independent causal effects.

|Tested upstream / downstream|Critical+ win rate|Duel-RL win rate|
|---|---:|---:|
`;
for(let u=5;u<=95;u+=5) {
  const gs=styles.map(s=>data.find(g=>g.condition.style===s&&g.condition.family==='100ms-fine-offset'&&g.condition.up_ms[0]===u));
  report+=`|${u} / ${100-u} ms|${pct(gs[0]).toFixed(2)}%|${pct(gs[1]).toFixed(2)}%|\n`;
}
report+=`
At 100 ms RTT, Critical+ ranged from 39.06% to 51.56% and Duel-RL from 42.19% to 51.56%. Changing the split by 5–10 ms affected performance. Shorter upstream or shorter downstream delay gave no consistent advantage. Small differences include finite-sample variation.

In the coarse sweep over all RTTs, Critical+ won 61.72% at 40 ms RTT (36 up/4 down) and 39.06% at 90 ms (81 up/9 down). Duel-RL won 63.28% at 70 ms (52.5 up/17.5 down) and 42.19% at 130 ms (97.5 up/32.5 down). Each opponent used an equal split at the same RTT. These are observed extremes among the tested conditions. The optimal split remains unknown.

## All pairings

Rows give the player's RTT, columns the opponent's RTT, and cells the row player's win percentage. Diagonal cells are 50% (128 wins, 128 losses). Transposed cells show the other side of the same matches and add no independent games.
`;
for(const s of styles) {
  report+=`
### ${name(s)} mirror matches

|Row / column RTT (ms)|${pings.join('|')}|
|---|${pings.map(()=>'---:').join('|')}|
`;
  for(const a of pings) report+=`|${a}|${pings.map(b=>rate(s,a,b).toFixed(1)).join('|')}|\n`;
}
report+=`
## In-match behavior and interpretation

The 2,560 traced matches were additional diagnostic reruns. All outcomes, HP values, and end times matched the corresponding untraced matches. At 10 vs 150 ms, mean successful attacks were 20.70 vs 19.81 for Critical+ and 24.80 vs 23.95 for Duel-RL. Mean critical hits were 15.54 vs 15.68 for Critical+ and 11.76 vs 11.09 for Duel-RL. Lower-ping Critical+ won more often despite landing fewer critical hits, so critical-hit count alone cannot explain the win rate.

Upstream delay postpones server receipt of attacks and movement. Downstream delay postpones client receipt of opponent information and other updates. Both interact with 50 ms ticks, roughly 16.67 ms receive frames, position interpolation, knockback, and attack charge. The nonmonotonic results are consistent with timing effects. This experiment did not estimate individual contributions. Diagnostic conditions recorded zero server attack rejections for range or similar checks and zero damage-invulnerability rejections, giving no evidence for increased rejections as the cause.

## Conditions and reproduction

- 64 shared conditions × two seats × two processing orders for simultaneous inputs = 256 matches per condition. Reuse the same 64 starting conditions across all pings and splits.
- Starting distance 3.5–32 blocks, lateral offset −2 to 2 blocks. A fixed seed generates server and both client tick phases in [0, 50 ms) and receive-frame phases within one frame. Both clients use 60 FPS.
- Start at 20 HP with ordinary gear and finite durability, maximum 300 seconds. Policies and models were unchanged throughout. Network delays are constant with no jitter or packet loss.
- Per policy: 120 RTT grid conditions, 75 fixed-RTT splits, 19 detailed splits at 100 ms, and five diagnostic conditions = 219. Both policies total 438 conditions and 112,128 matches, including reruns. The main grid comparison covers 61,440 matches.
- The 64 conditions sample the simulator. Intervals omit error relative to live Minecraft and untested conditions. The reported extremes have no multiple-comparison adjustment.

\`\`\`powershell
.\\tools\\rust\\cargo.bat run --release --locked --offline -p pvp-rl --example ping_sweep
node tools/research/ping/analyze-ping.mjs
.\\tools\\research\\ping\\plot-ping.bat
\`\`\`

Raw data and individual matches: \`build/benchmarks/ping-sweep/results.json\`. The 64 reproducible conditions: \`contexts.json\`. All-condition summary: \`summary.csv\`. Episode hits, crits, damage, and similar counters were measured only in diagnostics. Zeros elsewhere indicate missing measurements.
`;
await fs.writeFile(new URL('docs/research/ping/ping-impact.md',root),report);
await fs.writeFile(new URL('report.md',out),report);
export const summary={matches:data.reduce((s,g)=>s+n(g),0),groups:data.length,report:'docs/research/ping/ping-impact.md',checks:'counts, symmetric controls, trace equivalence passed'};
