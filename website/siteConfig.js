const users = [
  {
    caption: 'User1',
    image: '/test-site/img/docusaurus.svg',
    infoLink: 'https://mozilla.org',
    pinned: true,
  },
];

const siteConfig = {
  title: 'Firefox Application Services' /* title for your website */,
  tagline: 'Beyond the browser',
  url: 'https://vladikoff.github.io/app-services-site' /* your website url */,
  baseUrl: '/app-services-site/' /* base url for your project */,
  projectName: 'app-services-site',
  headerLinks: [
    {doc: 'doc1', label: 'Docs'},
    {doc: 'doc4', label: 'API'},
    {page: 'help', label: 'Help'},
  ],
  disableHeaderTitle: true,
  useEnglishUrl: false,
  users,
  /* path to images for header/footer */
  headerIcon: 'img/app-services.svg',
  footerIcon: 'img/app-services.svg',
  favicon: 'img/favicon.png',
  /* colors for website */
  colors: {
    primaryColor: '#424c55',
    secondaryColor: '#7A838B',
  },
  // This copyright info is used in /core/Footer.js and blog rss/atom feeds.
  copyright:
    'Copyright © ' +
    new Date().getFullYear() +
    ' Firefox Application Services',
  // organizationName: 'deltice', // or set an env variable ORGANIZATION_NAME
  // projectName: 'test-site', // or set an env variable PROJECT_NAME
  highlight: {
    // Highlight.js theme to use for syntax highlighting in code blocks
    theme: 'default',
  },
  scripts: ['https://buttons.github.io/buttons.js'],
  // You may provide arbitrary config keys to be used as needed by your template.
  repoUrl: 'https://github.com/vladikoff/app-services-site',
  /* On page navigation for the current documentation page */
  // onPageNav: 'separate',
};

module.exports = siteConfig;
