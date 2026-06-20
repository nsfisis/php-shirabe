//! ref: composer/tests/Composer/Test/Downloader/DownloadManagerTest.php

// These mock IO and individual downloaders to drive DownloadManager's selection/download/
// update/remove logic; mocking is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO and individual downloaders to drive DownloadManager; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_set_get_downloader);
stub!(test_get_downloader_for_incorrectly_installed_package);
stub!(test_get_downloader_for_correctly_installed_dist_package);
stub!(test_get_downloader_for_incorrectly_installed_dist_package);
stub!(test_get_downloader_for_correctly_installed_source_package);
stub!(test_get_downloader_for_incorrectly_installed_source_package);
stub!(test_get_downloader_for_metapackage);
stub!(test_full_package_download);
stub!(test_full_package_download_failover);
stub!(test_bad_package_download);
stub!(test_dist_only_package_download);
stub!(test_source_only_package_download);
stub!(test_metapackage_package_download);
stub!(test_full_package_download_with_source_preferred);
stub!(test_dist_only_package_download_with_source_preferred);
stub!(test_source_only_package_download_with_source_preferred);
stub!(test_bad_package_download_with_source_preferred);
stub!(test_update_dist_with_equal_types);
stub!(test_update_dist_with_not_equal_types);
stub!(test_get_available_sources_update_sticks_to_same_source);
stub!(test_update_metapackage);
stub!(test_remove);
stub!(test_metapackage_remove);
stub!(test_install_preference_without_preference_dev);
stub!(test_install_preference_without_preference_no_dev);
stub!(test_install_preference_without_match_dev);
stub!(test_install_preference_without_match_no_dev);
stub!(test_install_preference_with_match_auto_dev);
stub!(test_install_preference_with_match_auto_no_dev);
stub!(test_install_preference_with_match_source);
stub!(test_install_preference_with_match_dist);
