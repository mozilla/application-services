/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

//! Merino is a cross-platform client library for Mozilla's Merino service.
//!
//! It provides a [`CuratedRecommendationsClient`](curated_recommendations::CuratedRecommendationsClient)
//! that fetches curated content recommendations (articles, stories) from the Merino backend,
//! powering features like Firefox's New Tab page.
//!
//! This crate uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate cross-platform
//! bindings for Android and other targets.

pub mod curated_recommendations;
uniffi::setup_scaffolding!("merino");
