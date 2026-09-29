import type {ReactNode} from 'react';
import Layout from '@theme/Layout';
import {translate} from '@docusaurus/Translate';
import Hero from '@site/src/components/Hero';
import NativeFeatures from '@site/src/components/NativeFeatures';
import CompatStrip from '@site/src/components/CompatStrip';
import Architecture from '@site/src/components/Architecture';
import Family from '@site/src/components/Family';
import Community from '@site/src/components/Community';

export default function Home(): ReactNode {
  return (
    <Layout
      title={translate({id: 'home.meta.title', message: 'The LLM gateway that keeps native power'})}
      description={translate({
        id: 'home.meta.description',
        message: 'Synapse is an open-source Rust LLM gateway: OpenAI-compatible on the outside, native Vertex AI and Jev routing on the inside.',
      })}>
      <Hero />
      <main>
        <NativeFeatures />
        <CompatStrip />
        <Architecture />
        <Family />
        <Community />
      </main>
    </Layout>
  );
}
