PRAGMA foreign_keys=ON;
PRAGMA synchronous=NORMAL;

CREATE TABLE logins(
    _id INTEGER PRIMARY KEY AUTOINCREMENT,
    hostname TEXT NOT NULL,
    httpRealm TEXT,
    formSubmitURL TEXT,
    usernameField TEXT NOT NULL,
    passwordField TEXT NOT NULL,
    encryptedUsername TEXT NOT NULL,
    encryptedPassword TEXT NOT NULL,
    guid TEXT UNIQUE NOT NULL,
    encType INTEGER NOT NULL,
    timeCreated INTEGER,
    timeLastUsed INTEGER,
    timePasswordChanged INTEGER,
    timesUsed INTEGER
);

CREATE INDEX login_hostname_formSubmitURL_index ON logins(hostname,formSubmitURL);
CREATE INDEX login_hostname_httpRealm_index ON logins(hostname,httpRealm);
CREATE INDEX login_hostname_index ON logins(hostname);
