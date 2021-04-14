/**
 * Returns true if browser supports HTML5 localStorage.
 */
function supportsHTML5Storage() {
    try {
        return 'localStorage' in window && window['localStorage'] !== null;
    } catch (e) {
        return false;
    }
}

/**
 * Event handler for when a tab is clicked.
 */
function onClickTab(event) {
    let target = event.currentTarget;
    let language = target.dataset.lang;

    const initialTargetOffset = target.offsetTop;
    const initialScrollPosition = window.scrollY;
    switchAllTabs(language);

    // Keep initial perceived scroll position after resizing
    // that may happen due to activation of multiple tabs in the same page.
    const finalTargetOffset = target.offsetTop;
    window.scrollTo({
        top: initialScrollPosition + (finalTargetOffset - initialTargetOffset)
    });
}

/**
 * Switches the displayed tab for the given container.
 *
 * :param container: The div containing both the tab bar and the individual tabs
 * as direct children.
 * :param language: The language to switch to.
 */
function switchTab(container, language) {
    const previouslyActiveTab = container.querySelector(".tabcontents .active");
    previouslyActiveTab && previouslyActiveTab.classList.remove("active");
    let tab = container.querySelector(`.tabcontents [data-lang="${language}"]`);
    tab && tab.classList.add("active");

    const previouslyActiveButton = container.querySelector(".tabbar .active");
    previouslyActiveButton && previouslyActiveButton.classList.remove("active");
    let button = container.querySelector(`.tabbar [data-lang="${language}"]`);
    button && button.classList.add("active");
}

/**
 * Switches all tabs on the page to the given language.
 *
 * :param language: The language to switch to.
 */
function switchAllTabs(language) {
    const containers = document.getElementsByClassName("tabs");
    for (let i = 0; i < containers.length; ++i) {
        switchTab(containers[i], language);
    }

    if (supportsHTML5Storage()) {
        localStorage.setItem("glean-preferred-language", language);
    }
}

/**
 * Opens all tabs on the page to the given language on page load.
 */
function openTabs() {
    if (!supportsHTML5Storage()) {
        return;
    }

    let containers = document.getElementsByClassName("tabs");
    for (let i = 0; i < containers.length; ++i) {
        // Create tabs for every language that has content
        let tabs = containers[i].children[0];
        let tabcontents = containers[i].children[1];
        for (let tabcontent of tabcontents.children) {
            let button = document.createElement("button");
            button.dataset.lang = tabcontent.dataset.lang;
            button.className = "tablinks";
            button.onclick = onClickTab;
            button.innerText = tabcontent.dataset.lang;
            tabs.appendChild(button);
        }
    }

    var language = localStorage.getItem("glean-preferred-language");
    if (language == null) {
        language = "Kotlin";
    }

    switchAllTabs(language);
}

openTabs()
