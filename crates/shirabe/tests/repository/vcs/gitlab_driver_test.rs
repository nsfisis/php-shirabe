//! ref: composer/tests/Composer/Test/Repository/Vcs/GitLabDriverTest.php

// All cases either mock the HttpDownloader/IO to return GitLab API responses (a real
// HttpDownloader reaches curl_multi_init, todo!()), or — for testSupports — rely on the
// gitlab-domains configured in setUp plus the openssl extension, which are not modeled.

macro_rules! gitlab_stub {
    ($name:ident) => {
        #[test]
        #[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
        fn $name() {
            todo!()
        }
    };
}

gitlab_stub!(test_initialize);
gitlab_stub!(test_initialize_public_project);
gitlab_stub!(test_initialize_public_project_as_anonymous);
gitlab_stub!(test_initialize_with_port_number);
gitlab_stub!(test_invalid_support_data);
gitlab_stub!(test_get_dist);
gitlab_stub!(test_get_source);
gitlab_stub!(test_get_source_given_public_project);
gitlab_stub!(test_get_tags);
gitlab_stub!(test_get_paginated_refs);
gitlab_stub!(test_get_branches);
gitlab_stub!(test_supports);
gitlab_stub!(test_gitlab_sub_directory);
gitlab_stub!(test_gitlab_sub_group);
gitlab_stub!(test_gitlab_sub_directory_sub_group);
gitlab_stub!(test_forwards_options);
gitlab_stub!(test_protocol_override_repository_url_generation);
