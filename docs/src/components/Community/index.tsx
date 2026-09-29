import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Translate from '@docusaurus/Translate';
import styles from './styles.module.css';

const REPO = 'https://github.com/sustentabilitas/synapse-gateway';

export default function Community(): ReactNode {
  return (
    <section className={`sy-section ${styles.community}`}>
      <div className="container">
        <p className="sy-eyebrow">
          <Translate id="home.community.eyebrow">Open source</Translate>
        </p>
        <h2 className="sy-section-title">
          <Translate id="home.community.title">Open, free and built in the open</Translate>
        </h2>
        <p className="sy-section-lead">
          <Translate id="home.community.lead">
            Synapse is licensed under MPL-2.0, so you can build it into commercial products. Every feature on this page is in the open-source code, with no paid tier, and improvements contributed back are welcome.
          </Translate>
        </p>
        <div className={styles.links}>
          <Link className="button button--primary button--lg" href={REPO}>
            GitHub
          </Link>
          <Link className="button button--secondary button--outline button--lg" to="/docs/contributing/">
            <Translate id="home.community.contribute">Contribute</Translate>
          </Link>
          <Link className="button button--secondary button--outline button--lg" href="https://crates.io/crates/synapse-gateway">
            crates.io
          </Link>
          <Link className="button button--secondary button--outline button--lg" href="https://hub.docker.com/r/sustentabilitas/synapse-gateway">
            Docker Hub
          </Link>
          <Link className="button button--secondary button--outline button--lg" to="/blog/">
            <Translate id="home.community.blog">Blog</Translate>
          </Link>
        </div>
      </div>
    </section>
  );
}
