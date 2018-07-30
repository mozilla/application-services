# Firefox Accounts Metrics
*Last updated 2018-07-30*

**This file is WIP**. In the future it will be a combined resource detailing data collection, metrics definitions, pipeline information, etc. For now it mainly documents metrics parameters (e.g. utm_*) that reliers pass to Firefox Accounts servers.

## How Your Choice of Integration Method Affects Our Metrics

You should start at [this page](https://mozilla.github.io/application-services/docs/accounts/welcome.html) for more general information on how to integrate with Firefox Accounts.

In general, reliers must choose to either (1) self-host the first step in the FxA authentication flow (e.g. the form capturing the user's email) or (2) use the flow/UX that has been developed by the FxA team (e.g. in an iframe hosted on your page).

For example, [about:welcome](about:welcome) self-hosts the form that captures the user's email address, then hands it off to Firefox Accounts servers to (among other things) determine if there is already an account associated with that address.

**If you plan to go a similar route and self-host your own form, we ask that you do the following so that we can properly track top-of-funnel metrics** (such as volume of form views) associated with the page that hosts your FxA entrypoint:

1. When the page that hosts your FxA entrypoint loads, have it make an XHR call to https://accounts.firefox.com/metrics-flow. The domain name of the request should match the FxA page that is being redirected to (e.g. https://accounts.firefox.com). You can use `fetch` to get this info.
2. Include the following query parameters in the above request (see chart below for descriptions):
  * `entrypoint`
  * `utm_source`
  * `utm_campaign`
  * `form_type`
  * An example: `https://accounts.firefox.com/metrics-flow?entrypoint=my_page&utm_source=my_referrer&utm_campaign=my_campaign&form_type=email`
3. The response to metrics-flow will be a JSON object that contains the fields `flowId` and `flowBeginTime`. These values will need to be propagated to FxA as query parameters, which can be done using hidden form fields with the names `flow_id` and `flow_begin_time`. You can see an example of how the activity-stream team did this by looking [here](https://hg.mozilla.org/releases/mozilla-beta/diff/5d6261b568c6/browser/extensions/activity-stream/content-src/components/StartupOverlay/StartupOverlay.jsx#l1.22) (link will have to be updated).

## Description of Metrics-Related Query Parameters
**Note these may not be all the parameters you need to pass for your integration to work!** A more exhaustive but [less detailed list can be found here.](https://github.com/mozilla/fxa-content-server/blob/549fc459b851088ea910da182e17e748fa157f26/docs/query-params.md#context) What is documented below are only those that are relevant for metrics analysis in (e.g.) amplitude.

Notes:
* The "Regex Used for Validation" column shows the regular expression that the value of each parameter must conform to in order for the measurement to register in our metrics pipeline. If the value does not conform, all events associated with that parameter will fail to be logged.

* You must have access to the mozilla amplitude account to see the example charts. If you are a Mozilla employee, you can email X at mozilla dot org for more information on gaining access to amplitude.

|Name|Description|Example Values|Regex Used for Validation|Amplitude Chart Example|
|----|-----------|-------------|------------------|-----------------|
|`entrypoint`|The specific page or browser UI element containing the first step of the FxA sign-in/sign-up process (e.g., enter email form)|`firstrun`| /^[\w.-]+$/|[Firstrun form views](https://analytics.amplitude.com/mozilla-corp/chart/n8cd9no)|
|`service`|The name of the FxA relier/service that the user is signing into|`sync`|/^(sync&#124;content-server&#124;none&#124;[0-9a-f]{16})$/|[Completed Registrations by Service](https://analytics.amplitude.com/mozilla-corp/chart/85v4c88)|
|`form_type`|For self-hosted forms only (see above) the type of form that the user submits to begin the FxA flow|either: `email` if the form captures the user's email, otherwise `other`|/^(email&#124;other)$/|NA|
|`utm_source`|Unambiguous identifier of site or browser UI element that linked to the page containing the beginning of the FxA sign-in/sign-up process |`blog.mozilla.org`|/^[\w.%-]+$/|[Registration form views segmented by utm_source](https://analytics.amplitude.com/mozilla-corp/chart/f5sz7kt)|
|`utm_campaign`|More general label for the campaign that the site is part of|`firstrun`|/^[\w.%-]+$/|TBD|
|`utm_content`|Could be used to ID what was clicked to bring user to the page, or to identify the cohort in an A/B test|`textlink`,`firstrun-cohort-a`|/^[\w.%-]+$/|TBD|
|`utm_term`|If coming from a search engine, what search terms were used|`firefox+accounts`|/^[\w.%-]+$/|TBD|
|`utm_medium`|What type of link was used to direct to the page, if it came through a marketing campaign|`email`, `cpc`|/^[\w.%-]+$/|TBD|
