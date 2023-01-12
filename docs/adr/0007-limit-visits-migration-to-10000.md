# Limit Visits Migrated to Places History in Firefox iOS

* Status: accepted
* Deciders: teshaq, mhammond, lougeniaC64, dnarcese
* Date: 2023-01-06

## Context and Problem Statement

[The Application-Services team removed a legacy implementation of history in Firefox-ios and replaced it with a maintained implementation that was powering Firefox Android.](https://mozilla-hub.atlassian.net/browse/SYNC-3086)

A significant part of the project is migrating users’ history from the old database to a new one. To measure risk, we ran a dry-run migration. A dry-run migration runs a background thread in the user’s application and attempts to migrate to a fake database. The dry-run was implemented purely to collect telemetry on the migration to evaluate risk. The results can be found in the [following Looker dashboard](https://mozilla.cloud.looker.com/dashboards/917?Submission+Date=28+day+ago+for+28+day&Channel=mozdata.firefox%5E_ios.metrics&Places+History+Migration+Migration+Error+Rate+Numerator=NOT+NULL). Below is a list of observations.

### Observations from Dry-Run Experiment
The following is a list of observations from the experiment:
- 5-6% of migrations do not end. This means for 5-6% of users, the application was terminated before migration ended. For a real migration, this would mean those users lose all of their history unless we attempt the migration multiple times.
- Out of the migrations that failed (the 5-6% mentioned above) 97% of those users had over 10,000 history visits.
- Out of migrations that do end, over 99% of migrations are successful.
  - This means that we are not experiencing many errors with the migration beyond the time it takes.
- The average for visits migrated is around 25,000 - 45,000 visits.
- The median for visits migrated is around 5,000-15,000 visits.
  - The difference between the average and the median suggests that we have many users with a large number of visits
- For migrations that did end, the following are the percentiles for how long it took (in milliseconds). We would like to emphasize that **the following only includes migrations that did end**
  - 10th percentile: 37 ms
  - 25th percentile: 80 ms
  - 50th percentile: 400 ms
  - 75th percentile: 2,500 ms (2.5 seconds)
  - 90th percentile: 6,400 ms (6.4 seconds)
  - 95th percentile: 11,000 ms (11 seconds)
  - 99th percentile: 25,000 ms (25 seconds)
  - 99.9th  percentile: 50,000 ms (50 seconds)

### Problem Statement
Given the observations from the dry-run experiment, the rest of the document examines an approach to answer the question: **How can we increase the rate of which migrations end, and simultaneously keep the user’s database size at a reasonable size?**

The user implication of keeping the rate of ended migrations high is that users keep their history, and can interact normally with the URL bar to search history, searching history in the history panel and navigating to sites they visited in the past.



The user implication of keeping a reasonable database size is that the database is less likely to lock on long queries. Meaning we reduce performance issues when users use the search bar, the history panel and when navigating to sites.

Finally, it’s important to note that power users and daily active users will be more likely to have large histories and thus:
- Power users are more likely to fail their migration.
- Power users are more likely to have performance issues with history and the search bar.
  - We saw a version of this with Favicons in an earlier incident, where users were coming across a significant number of database locks, crashing the app. This isn’t to say that the incident is directly related to this, however, large histories could contribute to the state we saw in the incident as it would take longer to run the queries.

## Decision Drivers <!-- optional -->

* We must not lose users’ recent history.
  * What is considered “recent” is not concretely defined. There is prior art, however:
    * [Firefox Sync in Android syncs 5000 visits.](https://github.com/mozilla/application-services/blob/c34de065f5773b529a0def773cffb29b45676bce/components/places/src/history_sync/mod.rs#L16)
    * [Firefox Desktop and Android impose limits on the size of the database, 75 MiB in Android](https://github.com/mozilla-mobile/firefox-android/blob/8077db4b55c1a8976ec130462767e3075b9cbeab/android-components/components/browser/storage-sync/src/main/java/mozilla/components/browser/storage/sync/PlacesHistoryStorageWorker.kt#L38)
    * [When importing history from chrome to Firefox, we only migrate the latest 2000 visits](https://searchfox.org/mozilla-central/rev/f40d29a11f2eb4685256b59934e637012ea6fb78/browser/app/profile/firefox.js#2134)
    * [Chrome only keeps a user’s history for 90 days](https://support.google.com/chrome/answer/95589)

* User’s experience with History must not regress, and ideally should improve.
  * User experience is tightly coupled with the size of the database. The larger the database, the longer queries take. The longer queries take, the longer it would take for a user to observe their searched history and the history panel.


## Considered Options

* Keep the migration as-is.
  * This option means that we have no limit. We will attempt to migrate all history for our users.
* Introduce a date-based limit on visits for the migration
  * This option means that we only migrate visits that occurred in the past X days/weeks/months etc
* Introduce a visit number-based limit for the migration
  * This option means we only migrate the latest X visits

## Decision Outcome
Chosen option: **Introduce a visit number-based limit for the migration**. This option was chosen because given our decision drivers:
1. We must not lose users’ recent history:
  * We have established in the results of the dry-run, that the majority of failed migrations were for users with a large number of visits.
  * By setting a reasonable limit, we can increase the likelihood the migration succeeds. We can set the limit to encompass “recent history” while choosing a limit that has an over 99% success rate.
2. User’s experience with History must not regress, and ideally should improve.
  * We have established in our decision driver that the user’s experience with history is coupled with the database size.
  * By setting a reasonable limit, we can keep the size of the history database controlled.
  * It's also worth noting that with the switch to the new implementation of history, we are also introducing a target size for the database. This means that we have maintenance logic that would compact the database and prune it if it grows beyond the target size.


### Positive Consequences
* The migration runs in a shorter time.
  * This means a higher chance of the migration succeeding, thus keeping the user’s recent history without loss.
* Users who have less than the selected limit, will still get all their history. More on this in the Suggested Limit section.
* We keep the size of the history database low.
  * This way users with more than the limit, will only keep their recent history.
  * Additionally, when we delete users’ history from the old database, the size of the data the app keeps will decrease dramatically.
  * Keeping the database size low means we lower the chance a user has performance issues with the database.


### Negative Consequences

The biggest negative consequence is that **Users with more visits than the limit, will lose visits**.
 * Since we would only keep the latest X visits for a user, if a user has Y visits, they would lose all of the Y-X visits (assuming Y is greater than X)
 * The effect here is mitigated by the observation that recent history is more important to users than older history. Unfortunately, we do not have any telemetry to demonstrate this claim, but it’s an assumption based on the existing limits on history imposed in Android and Desktop mentioned in the decision drivers section.


## Pros and Cons of the Other Options

### Keep the migration as-is

* Good because if the migration succeeds, users keep all their history.
* Bad, because it’s less likely for migrations to succeed.
* Bad, because even if the migration succeeds it causes the size of the database to be large if a user has a lot of history.
  * Large databases can cause a regression in performance.
  * Users with a lot of history will now have two large databases (the old and new ones) since we won’t delete the data in the old database right away to support downgrades.
* Bad, because it can take a long time for the migration to finish.
* Bad because until the migration is over users will experience the app without history.


### Introduce a date-based limit on visits

* Good, because we match users’ usage of the app.
  * Users that use the app more, will keep more of their history.
* Good, because it’s more likely that the migration ends because we set a limit
* Bad because it’s hard to predict how large user’s databases will be.
  * This is particularly important for Sync users. As Firefox-iOS syncs all your history, meaning if a user has many visits before the limit across multiple platforms, a large number of visits will be migrated.
* Bad, because users who haven’t used the app since the limit, will lose all their history
  * For example, if the limit is 3 months, a user who last used the app 3 months ago will suddenly lose all their history


## Suggested Limit
This section describes a suggested limit for the visits. Although it’s backed up with telemetry, the specific number is up for discussion. **It’s also important to note that statistical significance was not a part of the analysis. Migration has run for over 16,000 users and although that may not be a statistically significant representation of our population, it’s good enough input to make an educated suggestion.**

* First, we start by observing the distribution of visit counts. This will tell us how many of our users have between 0-10000 visits, 10000-20000, etc. We will identify that most of our users have less than 10,000 visits.
* Then, we will observe the dry-run migration ended rate based on the above buckets. We will observe that users with under 10,000 visits have a high chance of migration success.
* Finally, based on the analysis and prior art we’ll suggest 10,000 visits.

### User History Distribution
We will look at <https://mozilla.cloud.looker.com/looks/1078> which demonstrates a distribution of our users based on the number of history visits. Note that the chart is based on our release population.

#### Observations
* 67% of firefox-ios users have less than 10,000 visits
* There is a very long tail to the distribution, with 6% of users having over 100,000 visits.

### Dry-run Ending Rate by Visits
We will look at <https://mozilla.cloud.looker.com/looks/1081>. The chart demonstrates the rate at which migrations end by the number of visits. We bucket users in buckets of 10,000 visits.

#### Observations
* We observe that for users that have visits under 10,000, the success rate is over 99.6%.
* We observe that for users with over 100,000 visits, the success rate drops to 75~80%.
* Users in between, have success rates in between. For example, users with visits between 10,000 and 20,000 have a 98-99% success rate.
* All success rates for buckets beyond 20,000 visits drop under 96%.

### Suggestion
Based on the above, we’re suggesting a limit of 10,000 visits because
* 10,000 visits encompass the full history of 67% of our users.
* Migrations with under 10,000 visits have a success rate of over 99%.
* For users with over 10,000 visits, they still keep the latest 10,000 visits. The choice is reasonable considering:
  * [Sync only retrieves the latest 5000 visits in Android](https://github.com/mozilla/application-services/blob/c34de065f5773b529a0def773cffb29b45676bce/components/places/src/history_sync/mod.rs#L16)
  * [Migrating from Chrome to Firefox only migrates 2000 visits in Desktop](https://searchfox.org/mozilla-central/rev/f40d29a11f2eb4685256b59934e637012ea6fb78/browser/app/profile/firefox.js#2134)



## Links <!-- optional -->

* [Epic for moving iOS’s history implementation to application-services places](https://mozilla-hub.atlassian.net/browse/SYNC-3086)
* [Dry-run migration experiment](https://experimenter.services.mozilla.com/nimbus/running-a-dry-run-migration-to-the-application-services-history-in-firefox-ios)
* [Overall dry-run migration looker dashboard](https://mozilla.cloud.looker.com/dashboards/917?Submission+Date=28+day+ago+for+28+day&Channel=mozdata.firefox%5E_ios.metrics&Places+History+Migration+Migration+Error+Rate+Numerator=NOT+NULL)
* [Firefox iOS User distribution by history](https://mozilla.cloud.looker.com/looks/1078)
* [Migration Ended rate by User History](https://mozilla.cloud.looker.com/looks/1081)
* [Firefox Sync on Android only Syncs 5000 sites](https://github.com/mozilla/application-services/blob/c34de065f5773b529a0def773cffb29b45676bce/components/places/src/history_sync/mod.rs#L16)
* [Firefox Desktop Limits import from Chrome to 2000 visits](https://searchfox.org/mozilla-central/rev/f40d29a11f2eb4685256b59934e637012ea6fb78/browser/app/profile/firefox.js#2134)
* [Firefox Android limits the size of its `places.db` to 75MiB](https://github.com/mozilla-mobile/firefox-android/blob/8077db4b55c1a8976ec130462767e3075b9cbeab/android-components/components/browser/storage-sync/src/main/java/mozilla/components/browser/storage/sync/PlacesHistoryStorageWorker.kt#L38)
* [Chrome only keeps 90 days of history](https://support.google.com/chrome/answer/95589)
* [Performance incident in Firefox iOS](https://docs.google.com/document/d/1brA7BQIObLOOsdf_9lJW3VAPh68AzCu1xrgcpGDnPak/edit)
