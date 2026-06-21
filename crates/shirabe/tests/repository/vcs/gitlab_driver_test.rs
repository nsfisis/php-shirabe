//! ref: composer/tests/Composer/Test/Repository/Vcs/GitLabDriverTest.php

// All cases either mock the HttpDownloader/IO to return GitLab API responses (a real
// HttpDownloader reaches curl_multi_init, todo!()), or — for testSupports — rely on the
// gitlab-domains configured in setUp plus the openssl extension, which are not modeled.

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_initialize() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_initialize_public_project() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_initialize_public_project_as_anonymous() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_initialize_with_port_number() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_invalid_support_data() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_get_dist() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_get_source() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_get_source_given_public_project() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_get_tags() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_get_paginated_refs() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_get_branches() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_supports() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_gitlab_sub_directory() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_gitlab_sub_group() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_gitlab_sub_directory_sub_group() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_forwards_options() {
    todo!()
}

#[test]
#[ignore = "GitLabDriver tests mock HttpDownloader/IO (curl_multi_init todo!()) or need setUp gitlab-domains config"]
fn test_protocol_override_repository_url_generation() {
    todo!()
}
