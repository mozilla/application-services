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
            <a href="/application-services/blog">
              Blog
            </a>
          </div>
          <div>
            <h5>More</h5>
            <a href="https://github.com/mozilla/application-services">mozilla/application-services</a>
            <a href="https://github.com/mozilla/fxa">mozilla/fxa</a>
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
