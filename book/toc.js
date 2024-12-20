// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded affix "><a href="index.html">Application Services Rust Components</a></li><li class="chapter-item expanded "><a href="contributing.html"><strong aria-hidden="true">1.</strong> Contributing</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="building.html"><strong aria-hidden="true">1.1.</strong> Building</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="howtos/locally-published-components-in-fenix.html"><strong aria-hidden="true">1.1.1.</strong> How to use the local development autopublish flow for Fenix</a></li><li class="chapter-item expanded "><a href="howtos/locally-published-components-in-firefox-ios.html"><strong aria-hidden="true">1.1.2.</strong> How to use the local development autopublish flow for Firefox iOS</a></li><li class="chapter-item expanded "><a href="howtos/locally-published-components-in-focus-ios.html"><strong aria-hidden="true">1.1.3.</strong> How to use the local development flow for Focus for iOS</a></li><li class="chapter-item expanded "><a href="howtos/locally-building-jna.html"><strong aria-hidden="true">1.1.4.</strong> How to locally build JNA</a></li><li class="chapter-item expanded "><a href="howtos/branch-builds.html"><strong aria-hidden="true">1.1.5.</strong> Branch builds</a></li></ol></li><li class="chapter-item expanded "><a href="howtos/testing-a-rust-component.html"><strong aria-hidden="true">1.2.</strong> How to test Rust Components</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="howtos/smoke-testing-app-services.html"><strong aria-hidden="true">1.2.1.</strong> How to integration (smoke) test application-services</a></li><li class="chapter-item expanded "><a href="design/test-faster.html"><strong aria-hidden="true">1.2.2.</strong> Writing efficient tests</a></li><li class="chapter-item expanded "><a href="howtos/debug-sql.html"><strong aria-hidden="true">1.2.3.</strong> How to debug SQL/sqlite</a></li></ol></li><li class="chapter-item expanded "><a href="dependency-management.html"><strong aria-hidden="true">1.3.</strong> Dependency management</a></li><li class="chapter-item expanded "><a href="howtos/adding-a-new-component.html"><strong aria-hidden="true">1.4.</strong> How to add a new component</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="howtos/building-a-rust-component.html"><strong aria-hidden="true">1.4.1.</strong> How to build a new syncable component</a></li><li class="chapter-item expanded "><a href="naming-conventions.html"><strong aria-hidden="true">1.4.2.</strong> Naming Conventions</a></li><li class="chapter-item expanded "><a href="android-faqs.html"><strong aria-hidden="true">1.4.3.</strong> How to use Rust Components in Android</a></li></ol></li><li class="chapter-item expanded "><a href="howtos/breaking-changes.html"><strong aria-hidden="true">1.5.</strong> Breaking API changes</a></li><li class="chapter-item expanded "><a href="howtos/vendoring-into-mozilla-central.html"><strong aria-hidden="true">1.6.</strong> How to vendor application-services into mozilla-central</a></li><li class="chapter-item expanded "><a href="logging.html"><strong aria-hidden="true">1.7.</strong> Logging</a></li><li class="chapter-item expanded "><a href="howtos/uniffi-object-destruction-on-kotlin.html"><strong aria-hidden="true">1.8.</strong> UniFFI Object Destruction on Kotlin</a></li></ol></li><li class="chapter-item expanded "><a href="adr/index.html"><strong aria-hidden="true">2.</strong> Architectural Decision Records</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="adr/0000-use-markdown-architectural-decision-records.html"><strong aria-hidden="true">2.1.</strong> ADR-0000</a></li><li class="chapter-item expanded "><a href="adr/0001-update-logins-api.html"><strong aria-hidden="true">2.2.</strong> ADR-0001</a></li><li class="chapter-item expanded "><a href="adr/0002-database-corruption.html"><strong aria-hidden="true">2.3.</strong> ADR-0002</a></li><li class="chapter-item expanded "><a href="adr/0003-swift-packaging.html"><strong aria-hidden="true">2.4.</strong> ADR-0003</a></li><li class="chapter-item expanded "><a href="adr/0004-early-startup-experiments.html"><strong aria-hidden="true">2.5.</strong> ADR-0004</a></li><li class="chapter-item expanded "><a href="adr/0005-remote-settings-client.html"><strong aria-hidden="true">2.6.</strong> ADR-0005</a></li><li class="chapter-item expanded "><a href="adr/0007-limit-visits-migration-to-10000.html"><strong aria-hidden="true">2.7.</strong> ADR-0007</a></li></ol></li><li class="chapter-item expanded "><a href="design/index.html"><strong aria-hidden="true">3.</strong> Design</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="design/megazords.html"><strong aria-hidden="true">3.1.</strong> Megazords</a></li><li class="chapter-item expanded "><a href="design/sync-manager.html"><strong aria-hidden="true">3.2.</strong> Sync Manager</a></li><li class="chapter-item expanded "><a href="design/sync-overview.html"><strong aria-hidden="true">3.3.</strong> Sync overview</a></li><li class="chapter-item expanded "><a href="design/swift-package-manager.html"><strong aria-hidden="true">3.4.</strong> Shipping Rust Components as Swift Packages</a></li><li class="chapter-item expanded "><a href="design/components-strategy.html"><strong aria-hidden="true">3.5.</strong> Rust Component&#39;s Strategy</a></li><li class="chapter-item expanded "><a href="design/metrics.html"><strong aria-hidden="true">3.6.</strong> Metrics - (Glean Telemetry)</a></li><li class="chapter-item expanded "><a href="design/rust-versions.html"><strong aria-hidden="true">3.7.</strong> Rust Version Policy</a></li><li class="chapter-item expanded "><a href="design/db-pragmas.html"><strong aria-hidden="true">3.8.</strong> Sqlite Database Pragma Usage</a></li></ol></li><li class="chapter-item expanded "><a href="howtos/releases.html"><strong aria-hidden="true">4.</strong> Releases</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="build-and-publish-pipeline.html"><strong aria-hidden="true">4.1.</strong> CI Publishing tools and flow</a></li><li class="chapter-item expanded "><a href="howtos/upgrading-nss-guide.html"><strong aria-hidden="true">4.2.</strong> How to upgrade NSS</a></li></ol></li><li class="chapter-item expanded "><a href="rust-docs/index.html"><strong aria-hidden="true">5.</strong> Rustdocs for components</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="rust-docs/as_ohttp_client/index.html"><strong aria-hidden="true">5.1.</strong> as_ohttp_client</a></li><li class="chapter-item expanded "><a href="rust-docs/autofill/index.html"><strong aria-hidden="true">5.2.</strong> autofill</a></li><li class="chapter-item expanded "><a href="rust-docs/crashtest/index.html"><strong aria-hidden="true">5.3.</strong> crashtest</a></li><li class="chapter-item expanded "><a href="rust-docs/fxa_client/index.html"><strong aria-hidden="true">5.4.</strong> fxa_client</a></li><li class="chapter-item expanded "><a href="rust-docs/logins/index.html"><strong aria-hidden="true">5.5.</strong> logins</a></li><li class="chapter-item expanded "><a href="rust-docs/nimbus/index.html"><strong aria-hidden="true">5.6.</strong> nimbus</a></li><li class="chapter-item expanded "><a href="rust-docs/places/index.html"><strong aria-hidden="true">5.7.</strong> places</a></li><li class="chapter-item expanded "><a href="rust-docs/push/index.html"><strong aria-hidden="true">5.8.</strong> push</a></li><li class="chapter-item expanded "><a href="rust-docs/remote_settings/index.html"><strong aria-hidden="true">5.9.</strong> remote_settings</a></li><li class="chapter-item expanded "><a href="rust-docs/relevancy/index.html"><strong aria-hidden="true">5.10.</strong> relevancy</a></li><li class="chapter-item expanded "><a href="rust-docs/search/index.html"><strong aria-hidden="true">5.11.</strong> search</a></li><li class="chapter-item expanded "><a href="rust-docs/suggest/index.html"><strong aria-hidden="true">5.12.</strong> suggest</a></li><li class="chapter-item expanded "><a href="rust-docs/sync15/index.html"><strong aria-hidden="true">5.13.</strong> sync15</a></li><li class="chapter-item expanded "><a href="rust-docs/tabs/index.html"><strong aria-hidden="true">5.14.</strong> tabs</a></li><li class="chapter-item expanded "><a href="rust-docs/viaduct/index.html"><strong aria-hidden="true">5.15.</strong> viaduct</a></li><li class="chapter-item expanded "><a href="rust-docs/webext_storage/index.html"><strong aria-hidden="true">5.16.</strong> webext_storage</a></li></ol></li><li class="chapter-item expanded "><a href="adding-docs.html"><strong aria-hidden="true">6.</strong> Adding to these documents</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString();
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
