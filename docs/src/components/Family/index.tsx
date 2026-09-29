import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
import styles from '../NativeFeatures/styles.module.css';

type Member = {name: string; description: string; to: string};

function members(): Member[] {
  return [
    {
      name: 'synapse-proxy',
      description: translate({
        id: 'home.family.proxy',
        message: 'A config-driven reverse-proxy sidecar: path-prefix routing, context injection, request and response transforms, streaming passthrough.',
      }),
      to: '/docs/synapse-family/proxy/overview/',
    },
    {
      name: 'synapse-a2a',
      description: translate({
        id: 'home.family.a2a',
        message: 'An in-memory A2A agent registry on the gateway’s API port: register agents, then let clients discover them, fetch their agent cards and resolve their URLs.',
      }),
      to: '/docs/synapse-family/a2a/overview/',
    },
    {
      name: 'synapse-mcp',
      description: translate({
        id: 'home.family.mcp',
        message: 'An MCP gateway that routes tool calls to servers registered on demand and injects the caller’s tenant identity, so tools see who is calling.',
      }),
      to: '/docs/synapse-family/mcp/overview/',
    },
  ];
}

export default function Family(): ReactNode {
  return (
    <section className="sy-section sy-section--alt">
      <div className="container">
        <p className="sy-eyebrow">
          <Translate id="home.family.eyebrow">The Synapse family</Translate>
        </p>
        <h2 className="sy-section-title">
          <Translate id="home.family.title">More than a gateway</Translate>
        </h2>
        <div className={styles.grid}>
          {members().map((member) => (
            <Link key={member.to} className="sy-card" to={member.to}>
              <h3>
                <code>{member.name}</code>
              </h3>
              <p>{member.description}</p>
            </Link>
          ))}
        </div>
      </div>
    </section>
  );
}
