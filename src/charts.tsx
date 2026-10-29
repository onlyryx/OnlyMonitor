import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { formatBytes, type HistoryPoint } from './history';

export function TrendChart({ history, end, metric, compact = false }: { history: HistoryPoint[]; end: number; metric: 'cpu' | 'memory' | 'network'; compact?: boolean }) {
  const network = metric === 'network';
  const labels = { cpu: 'CPU 利用率', memory: '内存使用率', network: '上传与下载速度' };
  return <div className={`cpu-chart ${compact ? 'mini-chart' : ''} ${network ? 'network-chart' : ''}`} role="img" aria-label={`最近60秒${labels[metric]}曲线`}>
    <ResponsiveContainer width="100%" height="100%" minWidth={0}>
      <AreaChart data={history} margin={{ top: 12, right: 10, left: network ? 6 : -26, bottom: 0 }}>
        <defs><linearGradient id={`fill-${metric}`} x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="#718b40" stopOpacity={0.24} /><stop offset="100%" stopColor="#718b40" stopOpacity={0.01} /></linearGradient></defs>
        <CartesianGrid vertical={false} stroke="#25332918" strokeDasharray="4 5" />
        <XAxis dataKey="time" type="number" allowDataOverflow domain={[end - 60000, end]} ticks={[end - 60000, end - 30000, end]} tickFormatter={value => Number(value) === end ? '现在' : `${Math.round((end - Number(value)) / 1000)}s`} tickLine={false} axisLine={false} tick={{ fill: '#59645a', fontSize: 12, fontWeight: 500 }} />
        <YAxis domain={network ? [0, 'auto'] : [0, 100]} width={network ? 86 : 60} tickFormatter={value => network ? formatBytes(Number(value)) : `${value}`} tickLine={false} axisLine={false} tick={{ fill: '#59645a', fontSize: 12, fontWeight: 500 }} />
        <Tooltip labelFormatter={value => new Date(Number(value)).toLocaleTimeString('zh-CN', { hour12: false })} formatter={(value, name) => [network ? `${formatBytes(Number(value))}/s` : `${Number(value).toFixed(1)}%`, name === 'download' ? '下载' : name === 'upload' ? '上传' : labels[metric]]} contentStyle={{ background: '#fbfcf7', border: '1px solid #cbd2c5', borderRadius: 12, color: '#253329', fontSize: 13 }} />
        {network ? <><Area type="linear" dataKey="download" stroke="#46694f" fill="#46694f" fillOpacity={0.08} strokeWidth={2} isAnimationActive={false} connectNulls={false} dot={false} /><Area type="linear" dataKey="upload" stroke="#83983c" fill="#83983c" fillOpacity={0.07} strokeWidth={2} isAnimationActive={false} connectNulls={false} dot={false} /></> : <Area type="linear" dataKey={metric} stroke={metric === 'memory' ? '#5d8067' : '#718b40'} strokeWidth={2.3} fill={`url(#fill-${metric})`} isAnimationActive={false} connectNulls={false} dot={false} />}
      </AreaChart>
    </ResponsiveContainer>
  </div>;
}
