import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
import styles from './styles.module.css';

type Item = {label: string; to: string};

function items(): Item[] {
  return [
    {label: translate({id: 'home.compat.openai', message: 'OpenAI SDKs work unchanged'}), to: '/docs/reference/http-api/'},
    {label: translate({id: 'home.compat.fallback', message: 'Fallback across OpenAI, Qwen, vLLM, Ollama and TGI'}), to: '/docs/configuration/providers/'},
    {label: translate({id: 'home.compat.ledger', message: 'Per-tenant cost ledger'}), to: '/docs/guides/cost-ledger/'},
    {label: translate({id: 'home.compat.otel', message: 'OpenTelemetry metrics and a Grafana dashboard'}), to: '/docs/operating/metrics/'},
    {label: translate({id: 'home.compat.guardrails', message: 'Input guardrails'}), to: '/docs/configuration/guardrails-policy/'},
  ];
}

export default function CompatStrip(): ReactNode {
  return (
    <section className="sy-section sy-section--alt">
      <div className="container">
        <p className="sy-eyebrow">
          <Translate id="home.compat.eyebrow">And everything you expect</Translate>
        </p>
        <ul className={styles.strip}>
          {items().map((item) => (
            <li key={item.to}>
              <Link to={item.to}>{item.label}</Link>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
