/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines functions for testing if an environment from the
//! configuration matches the user environment.

use crate::{JSONVariantEnvironment, SearchApplicationName, SearchUserEnvironment};

/// Matches the user's environment against the given environment from the
/// configuration.
///
/// This function expects the locale, region and app version in the environment
/// to be lower case.
pub(crate) fn matches_user_environment(
    environment: &JSONVariantEnvironment,
    user_environment: &SearchUserEnvironment,
) -> bool {
    if !environment.excluded_distributions.is_empty()
        && environment
            .excluded_distributions
            .contains(&user_environment.distribution_id)
    {
        return false;
    }

    matches_region_and_locale(
        &user_environment.region,
        &user_environment.locale,
        environment,
    ) && matches_distribution(
        &user_environment.distribution_id,
        &environment.distributions,
    ) && matches_application(&user_environment.app_name, &environment.applications)
}

/// Determines whether the region and locale constraints in the supplied
/// environment apply to a user given the region and locale they are using.
fn matches_region_and_locale(
    user_region: &str,
    user_locale: &str,
    environment: &JSONVariantEnvironment,
) -> bool {
    if does_array_include(&environment.excluded_regions, user_region)
        || does_array_include(&environment.excluded_locales, user_locale)
    {
        return false;
    }

    // This is a special case, if all_regions_and_locales is false (default value)
    // and region and locales are not set, then we assume that for the purposes of
    // matching region & locale, we do match everywhere. This allows us to specify
    // none of these options but match against other items such as distribution or
    // application name.
    if !environment.all_regions_and_locales
        && environment.regions.is_empty()
        && environment.locales.is_empty()
    {
        return true;
    }

    if does_array_include(&environment.regions, user_region)
        && does_array_include(&environment.locales, user_locale)
    // && environment.all_regions_and_locales
    {
        return true;
    }

    if environment.regions.is_empty() && does_array_include(&environment.locales, user_locale) {
        return true;
    }

    if environment.locales.is_empty() && does_array_include(&environment.regions, user_region) {
        return true;
    }

    if environment.all_regions_and_locales {
        return true;
    }

    false
}

fn matches_distribution(user_distribution_id: &str, environment_distributions: &[String]) -> bool {
    environment_distributions.is_empty()
        || environment_distributions.contains(&user_distribution_id.to_string())
}

fn matches_application(
    user_application_name: &SearchApplicationName,
    environment_applications: &[SearchApplicationName],
) -> bool {
    environment_applications.is_empty() || environment_applications.contains(user_application_name)
}

fn does_array_include(config_array: &[String], compare_item: &str) -> bool {
    !config_array.is_empty()
        && config_array
            .iter()
            .any(|x| x.to_lowercase() == compare_item)
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::*;

    #[test]
    fn test_matches_user_environment_all_locales() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "FR".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when all_regions_and_locales is true"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when all_regions_and_locales is false (default) and no regions/locales are specified"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec!["fi".to_string()],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when all_regions_and_locales is true and the locale is excluded"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec!["FI".to_string()],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when all_regions_and_locales is true and the excluded locale is a different case"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec!["en-US".to_string()],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when all_regions_and_locales is true and the locale is not excluded"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec![],
                    excluded_regions: vec!["us".to_string(), "fr".to_string()],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when all_regions_and_locales is true and the region is excluded"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec![],
                    excluded_regions: vec!["US".to_string(), "FR".to_string()],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when all_regions_and_locales is true and the excluded region is a different case"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    excluded_locales: vec![],
                    excluded_regions: vec!["us".to_string()],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when all_regions_and_locales is true and the region is not excluded"
        );
    }

    #[test]
    fn test_matches_user_environment_locales() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec!["en-gb".to_string(), "fi".to_string()],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the user locale matches one from the config"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec!["en-GB".to_string(), "FI".to_string()],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the user locale matches one from the config and is a different case"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec!["en-gb".to_string(), "en-ca".to_string()],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when the user locale does not match one from the config"
        );
    }

    #[test]
    fn test_matches_user_environment_regions() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["gb".to_string(), "fr".to_string()],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the user region matches one from the config"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["GB".to_string(), "FR".to_string()],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the user region matches one from the config and is a different case"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec!["gb".to_string(), "ca".to_string()],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when the user region does not match one from the config"
        );
    }

    #[test]
    fn test_matches_user_environment_locales_with_excluded_regions() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec!["gb".to_string(), "ca".to_string()],
                    locales: vec!["en-gb".to_string(), "fi".to_string()],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the locale matches and the region is not excluded"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec!["gb".to_string(), "fr".to_string()],
                    locales: vec!["en-gb".to_string(), "fi".to_string()],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when the locale matches and the region is excluded"
        );
    }

    #[test]
    fn test_matches_user_environment_regions_with_excluded_locales() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec!["en-gb".to_string(), "de".to_string()],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["gb".to_string(), "fr".to_string()],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the region matches and the locale is not excluded"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec!["en-gb".to_string(), "fi".to_string()],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["gb".to_string(), "fr".to_string()],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when the region matches and the locale is excluded"
        );
    }

    #[test]
    fn test_matches_user_environment_distributions() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec!["distro-1".to_string()],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-1".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the distribution matches one in the environment"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec!["distro-2".to_string(), "distro-3".to_string()],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-3".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the distribution matches one in the environment when there are multiple"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec!["distro-2".to_string(), "distro-3".to_string()],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-4".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when the distribution does not match any in the environment"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["fr".to_string()],
                    distributions: vec!["distro-1".to_string(), "distro-2".to_string()],
                    excluded_distributions: vec![],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-2".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the distribution and region matches the environment"
        );
    }

    #[test]
    fn test_matches_user_environment_excluded_distributions() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["fr".to_string()],
                    distributions: vec!["distro-1".to_string(), "distro-2".to_string()],
                    excluded_distributions: vec!["
                        distro-3".to_string(), "distro-4".to_string()
                    ],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-2".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the distribution matches the distribution list but not the excluded distributions"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["fr".to_string()],
                    distributions: vec!["distro-1".to_string(), "distro-2".to_string()],
                    excluded_distributions: vec!["distro-3".to_string(), "distro-4".to_string()],
                    applications: vec![],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-3".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return false when the distribution matches the the excluded distributions"
        );
    }

    #[test]
    fn test_matches_user_environment_application_name() {
        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![SearchApplicationName::Firefox],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the application name matches the one in the environment"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![
                        SearchApplicationName::FirefoxAndroid,
                        SearchApplicationName::Firefox
                    ],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-3".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the application name matches one in the environment when there are multiple"
        );

        assert!(
            !matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec![],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![
                        SearchApplicationName::FirefoxAndroid,
                        SearchApplicationName::Firefox
                    ],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: "distro-4".into(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::FirefoxIos,
                    version: String::new(),
                }
            ),
            "Should return false when the distribution does not match any in the environment"
        );

        assert!(
            matches_user_environment(
                &crate::JSONVariantEnvironment {
                    all_regions_and_locales: false,
                    excluded_locales: vec![],
                    excluded_regions: vec![],
                    locales: vec![],
                    regions: vec!["fr".to_string()],
                    distributions: vec![],
                    excluded_distributions: vec![],
                    applications: vec![
                        SearchApplicationName::FirefoxAndroid,
                        SearchApplicationName::Firefox
                    ],
                },
                &SearchUserEnvironment {
                    locale: "fi".into(),
                    region: "fr".into(),
                    update_channel: SearchUpdateChannel::Default,
                    distribution_id: String::new(),
                    experiment: String::new(),
                    app_name: SearchApplicationName::Firefox,
                    version: String::new(),
                }
            ),
            "Should return true when the distribution and region matches the environment"
        );
    }
}
