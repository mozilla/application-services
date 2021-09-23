# Running experiments on first run early startup

* Status: rejected
* Deciders: teshaq, travis, k88hudson, jhugman, jaredlockhart
* Date: 2021-08-16

Technical Story: https://mozilla-hub.atlassian.net/browse/SDK-323

## Context and Problem Statement

As an experimenter, I would like to run experiments early on a user's first run of the application. However, the experiment data is only available on the second run. We would like to have that experiment data available before the user's first run.
For more information: https://docs.google.com/document/d/1Qw36_7G6XyHvJZdM-Hxh4nqYZyCsYajG0L5mO33Yd5M/edit

## Decision Drivers

* Availability of experiments early on the first run
* No impact on experimentation data analysis
* Flexibility in creating experiments
* Ability to quickly disable experiments
* Simplicity of releases
* Mobile's expectations of Nimbus (The SDK should be idempotent)

## Considered Options

* **(A) Do Nothing**
    * Keep everything the way it is, preventing us from experimenting on users early on their first run
* **(B) Bundle Experiment data with app on release**
    * On release, have an `initial_experiments.json` that defines the experiments that will be applied early on the first run
    * Later on the first run, the client would retrieve the actual experiment data from remote-settings and overwrite the bundled data
* **(C) Retrieve Experiment data on first run, and deal with delay**
    * We can retrieve the experiment data on the first run, experiment data however will not be available until after a short delay (network I/O + some disk I/O)

## Decision Outcome

None of the options were feasible, so for now we are sticking with option **(A) Do Nothing** until there are experiments planned that are expected to run on early startup on the first run, then we will revaluate our options.

The **(B) Bundle Experiment data with app on release** option was rejected mainly due to difficulty in disabling experiments and pausing enrollments. This can create a negative user experience as it prevents us from disabling any problematic experiments. Additionally, it ties experiment creation with application release cycles.

The **(C) Retrieve Experiment data on first run, and deal with delay** option was rejected due to the fact it changes the Nimbus SDK will no longer be idempotent,and the possibility of introducing undesirable UI.

## Pros and Cons of the Options

### Do nothing

* Good, because it keeps the flexibility in experiment creation
* Good, because disabling experiments can still done remotely for all experiments
* Good, because it keeps the Nimbus SDK idempotent.
* Bad, because it doesn't address the main problem of exposing experiments to user on their first run

### Bundle Experiment data with app on release
* Good, because it allows us to run experiments early on a user's first run
* Good, because it prevents us from having to wait for experiments, especially if a user has a slow network connection
* Bad, because it ties experiment creation with release cycles
* Bad, because it prevents us from disabling problematic first-run experiments without a dot release
* Bad, because it prevents us from pausing enrollment on first-run experiments without a dot release
* Bad, because it requires investment from the console team, and can modify existing flows.

### Retrieve Experiment data on first run, and deal with delay 

* Good, because it enables us to retrieve experiments for users on their first run
* Good, because it keeps the flexibility in experiment creation
* Good, because disabling experiments can still done remotely for all experiments
* Bad, because experiments may not be ready early on the user's experience
* Bad, because it forces the customer application to deal with either the delay, or changing the configuration shortly after startup. e.g. a loading spinner or a pre-onboarding screen not under experimental control; delaying initialization of onboarding screens until after experiments have been loaded.
* Bad, because it changes the programming model from Nimbus being an idempotent configuration store to configuration changing non-deterministically.
* Bad, because the experimentation platform could force the app to add unchangeable user interface for the entire population. This itself may have an effect on key metrics.

## Links

* [RFC for bundling into iOS and Fenix](https://docs.google.com/document/d/1Qw36_7G6XyHvJZdM-Hxh4nqYZyCsYajG0L5mO33Yd5M/edit#heading=h.b1n8hquqkety)
* Document presented to product managers about **(C) Retrieve Experiment data on first run, and deal with delay**: https://docs.google.com/document/d/1X1hC3t5zC7-Rp0OPIoiUr_ueLOAI0ez_jqslaNzOHjY/edit
* Demo presenting option **(C) Retrieve Experiment data on first run, and deal with delay**: https://drive.google.com/file/d/19HwnlwrabmSNsB7tjW2l4kZD3PWABi4u/view?usp=sharing
