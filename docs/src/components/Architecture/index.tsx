import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
import styles from './styles.module.css';

type Box = {x: number; y: number; w: number; label: string; accent?: boolean};

function Node({x, y, w, label, accent}: Box): ReactNode {
  return (
    <g>
      <rect x={x} y={y} width={w} height={56} rx={10} className={accent ? styles.nodeAccent : styles.node} />
      <text x={x + w / 2} y={y + 33} textAnchor="middle" className={styles.label}>
        {label}
      </text>
    </g>
  );
}

function Arrow({d}: {d: string}): ReactNode {
  return <path d={d} className={styles.arrow} markerEnd="url(#sy-arrow)" />;
}

export default function Architecture(): ReactNode {
  const lanes = [
    {y: 40, label: translate({id: 'home.arch.lane.standard', message: 'Standard lane'}), provider: 'OpenAI · Qwen · vLLM'},
    {y: 152, label: translate({id: 'home.arch.lane.vertex', message: 'Native Vertex lane'}), provider: 'Vertex AI', accent: true},
    {y: 264, label: translate({id: 'home.arch.lane.jev', message: 'Jev lane'}), provider: 'TypeSafe', accent: true},
  ];
  return (
    <section className="sy-section">
      <div className="container">
        <p className="sy-eyebrow">
          <Translate id="home.arch.eyebrow">Architecture</Translate>
        </p>
        <h2 id="sy-arch-title" className="sy-section-title">
          <Translate id="home.arch.title">Three lanes, one OpenAI-compatible front door</Translate>
        </h2>
        <p className="sy-section-lead">
          <Translate id="home.arch.lead">
            Every request passes guardrails, then lane detection picks the standard, native Vertex or Jev lane. Each route is an ordered fallback chain, and every call lands in the cost ledger and the metrics pipeline.
          </Translate>{' '}
          <Link to="/docs/overview/architecture/">
            <Translate id="home.arch.more">Read the architecture guide</Translate>
          </Link>
        </p>
        <div className={styles.scroller} role="region" aria-labelledby="sy-arch-title" tabIndex={0}>
          <svg viewBox="0 0 800 400" className={styles.diagram} role="img" aria-label={translate({id: 'home.arch.aria', message: 'Synapse request flow across three lanes'})}>
            <defs>
              <marker id="sy-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                <path d="M0 0 L10 5 L0 10 z" className={styles.arrowHead} />
              </marker>
            </defs>
            <Node x={10} y={152} w={90} label={translate({id: 'home.arch.client', message: 'Client'})} />
            <Node x={130} y={152} w={110} label={translate({id: 'home.arch.guardrails', message: 'Guardrails'})} />
            <Node x={270} y={152} w={140} label={translate({id: 'home.arch.detect', message: 'Lane detection'})} />
            <Arrow d="M100 180 H128" />
            <Arrow d="M240 180 H268" />
            {lanes.map((lane) => (
              <g key={lane.y}>
                <Arrow d={`M410 180 C425 180 425 ${lane.y + 28} 438 ${lane.y + 28}`} />
                <Node x={440} y={lane.y} w={160} label={lane.label} accent={lane.accent} />
                <Arrow d={`M600 ${lane.y + 28} H628`} />
                <Node x={630} y={lane.y} w={160} label={lane.provider} />
              </g>
            ))}
            <rect x={10} y={344} width={780} height={40} rx={8} className={styles.bar} />
            <text x={400} y={369} textAnchor="middle" className={styles.barLabel}>
              {translate({id: 'home.arch.bar', message: 'Fallback chains · Cost ledger · OpenTelemetry metrics'})}
            </text>
          </svg>
        </div>
      </div>
    </section>
  );
}
