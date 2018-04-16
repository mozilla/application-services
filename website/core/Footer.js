const React = require('react');

class Footer extends React.Component {
  docUrl(doc, language) {
    const baseUrl = this.props.config.baseUrl;
    return baseUrl + 'docs/' + '' + doc;
  }

  pageUrl(doc, language) {
    const baseUrl = this.props.config.baseUrl;
    return baseUrl + '' + doc;
  }

  render() {
    return (
      <footer className="productShowcaseSection nav-footer"  id="footer">
        <section className="sitemap">
          <div>
            <h5>Docs</h5>
            <a href={this.docUrl('accounts/welcome.html', this.props.language)}>
              Firefox Accounts
            </a>
          </div>
          <div>
            <h5>Community</h5>
            <a href={this.pageUrl('users.html', this.props.language)}>
              User Showcase
            </a>
            <a
              href="http://stackoverflow.com/questions/tagged/"
              target="_blank">
              Stack Overflow
            </a>
            <a href="https://discordapp.com/">Project Chat</a>
            <a href="https://twitter.com/" target="_blank">
              Twitter
            </a>
          </div>
          <div>
            <h5>More</h5>
            <a href="https://github.com/mozilla">github/mozilla</a>
            <a href="https://github.com/mozilla-services">github/mozilla-services</a>
          </div>
        </section>

        <section className="copyright">
          Firefox Application Services
        </section>
      </footer>
    );
  }
}

module.exports = Footer;
