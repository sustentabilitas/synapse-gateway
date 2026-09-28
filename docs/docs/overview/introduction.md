---
sidebar_position: 1
title: What is Synapse?
description: Synapse is an open-source Rust LLM gateway that is OpenAI-compatible on the outside and keeps native Vertex AI and Jev routing on the inside.
---

# What is Synapse?

Synapse is an open-source LLM gateway written in Rust. It accepts standard OpenAI
`POST /v1/chat/completions` requests and routes them through config-driven fallback
chains, while keeping a native Vertex AI lane and a TypeSafe Jev lane for the
features generic adapters throw away.
