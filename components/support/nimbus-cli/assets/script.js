function onReload() {
    const uaString = window.navigator.userAgent.toLowerCase();
    let el;
    if (uaString.indexOf("iphone") >= 0) {
        el = document.getElementById("ios-latest");
    } else if (uaString.indexOf("android") >= 0) {
        el = document.getElementById("android-latest");
    } else {
        el = document.getElementById("web-latest");
    }

    const button = document.getElementById("the-only-button");
    if (el) {
        button.textContent = String.fromCodePoint(0x25B6);
        el.click()
    } else {
        button.textContent = String.fromCodePoint(0x1F504);
    }
}

function onClick() {
    window.location.reload();
}

window.addEventListener("DOMContentLoaded", (event) => {
    console.log("DOM fully loaded and parsed");
    onReload()
});
