---
id: concepts
title: Core Concepts
sidebar_label: Core Concepts
---

## Components

The application-services team publishes a suite of re-usable client components
that applications can use to integrate with services such as Firefox Accounts
and Firefox Sync.

TODO: link to list of components, API docs, etc.

## Megazords

Since the application-services components are built using Rust and compiled to
native code, trying to consume multiple components individually would have
several downsides:

* Multiple copies of the rust standard library
* Multiple copies of shared dependencies
* Duplicated setup and potential conflicts in global infrastructure such as
  logging.

To avoid these downsides, and to gain the benefits of cross-component native-code
Link Time Optimization (LTO, i.e., inlining, dead code elimination, etc), we
recommend components be consumed through a *megazord library*.  These are
pre-built aggregate artifacts in which multiple components have been
compiled together as a single unit.

For initial development, it's likely easiest to use a megazord library that
includes all available components. Once your app is ready for final release,
reach out in #rust-components on Slack to discuss a custom megazord build
with only the components you need.