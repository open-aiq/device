# Handout: Fix URL parser bug in arduino-esp32 `HTTPClient`

**Status:** not started · **Target repo:** [espressif/arduino-esp32](https://github.com/espressif/arduino-esp32) · **Discovered:** 2026-06-12, while debugging this firmware's `api.ipify.org` request

---

## 1. The bug in one paragraph

`HTTPClient::beginInternal()` extracts the hostname from a URL by searching for the
first `/` only. Per RFC 3986 §3.2, the host component ends at the first `/`, `?`,
or `#`. So a legal URL with an empty path but a query string —
`https://api.ipify.org?format=json` — gets `api.ipify.org?format=json` extracted
as the hostname, which is then passed to DNS and fails with a misleading error
two layers down:

```
[E][WiFiGeneric.cpp:1583] hostByName(): DNS Failed for api.ipify.org?format=json
HTTP Error: -1 (connection refused)
```

Every spec-compliant client (curl, browsers, Python requests) normalizes this URL
to `GET /?format=json` + `Host: api.ipify.org`. Verified working:
`curl -v "https://api.ipify.org?format=json"` sends exactly that.

The same defect also mishandles fragments: `http://host#section` → hostname
`host#section`.

## 2. Exact location

- **File:** `libraries/HTTPClient/src/HTTPClient.cpp`
- **Function:** `bool HTTPClient::beginInternal(String url, const char* expectedProtocol)`
- **Line:** ~256 on master (as of 2026-06-13); line 269 in the local PlatformIO copy at
  `~/.platformio/packages/framework-arduinoespressif32/libraries/HTTPClient/src/HTTPClient.cpp`

Defective code:

```cpp
index = url.indexOf('/');          // <-- only '/' considered a host terminator
if (index == -1) {
    index = url.length();
    url += '/';
}
String host = url.substring(0, index);
url.remove(0, index); // remove host part
```

Note the `if (index == -1)` branch: that is the partial fix from
[issue #2190](https://github.com/espressif/arduino-esp32/issues/2190) (2018),
which handled `http://host` (no path at all) but not `http://host?query` or
`http://host#fragment`.

## 3. Proposed fix

Host ends at the first of `/`, `?`, `#`; whatever remains becomes the URI, with a
`/` prepended if it doesn't start with one (covers both the empty-path and the
`?query` cases):

```cpp
// RFC 3986 §3.2: the authority (host) component is terminated by the
// first '/', '?' or '#', whichever comes first.
int endOfHost = url.length();
for (unsigned int i = 0; i < url.length(); i++) {
    char c = url.charAt(i);
    if (c == '/' || c == '?' || c == '#') {
        endOfHost = i;
        break;
    }
}
String host = url.substring(0, endOfHost);
url.remove(0, endOfHost);            // remainder is the path + query (+ fragment)
if (!url.startsWith("/")) {
    url = "/" + url;                 // "" -> "/",  "?q=1" -> "/?q=1"
}
```

The rest of the function (auth `@` split, port `:` split, `_uri = url`) can stay
as-is — both operate on the now-correct `host` substring.

**Optional follow-up (separate commit or leave out of the PR):** strip the
fragment before sending, since `#fragment` is client-side only and must not
appear in the request line. `int hash = url.indexOf('#'); if (hash >= 0) url.remove(hash);`
after the prepend step.

## 4. Test matrix

| Input URL | Expected host | Expected URI | Current behavior |
|---|---|---|---|
| `http://host/path?q=1` | `host` | `/path?q=1` | OK (unchanged) |
| `http://host/` | `host` | `/` | OK (unchanged) |
| `http://host` | `host` | `/` | OK (fixed by #2190) |
| `http://host?q=1` | `host` | `/?q=1` | **BROKEN** — host=`host?q=1` |
| `http://host:8080?q=1` | `host`, port 8080 | `/?q=1` | **BROKEN** |
| `http://user:pw@host?q=1` | `host` + auth | `/?q=1` | **BROKEN** |
| `http://host#frag` | `host` | `/#frag` (or `/` with optional fix) | **BROKEN** — host=`host#frag` |
| `https://api.ipify.org?format=json` | `api.ipify.org` | `/?format=json` | **BROKEN** — the original symptom |

How to verify without a network: `_host`/`_uri` are protected members. Options:
- Quick local check: enable core debug level `DEBUG` and read the existing
  `log_d("protocol: %s, host: %s port: %d url: %s", ...)` line at the end of
  `beginInternal` — it prints exactly the four parsed components.
- For the PR: see if `tests/validation/` has an HTTP suite to extend; tests run
  via pytest-embedded on Wokwi (simulated WiFi — `pytest-embedded-wokwi`), QEMU,
  and real-hardware runners. A Wokwi test can hit a local HTTP endpoint and
  assert on serial output. Parser-only assertions could also use a small test
  subclass (`class TestableHTTPClient : public HTTPClient`) to expose the
  parsed fields, since members are protected, not private.

End-to-end sanity check on real hardware: this firmware itself —
`src/main.cpp` request to `https://api.ipify.org?format=json` (slash-less form)
should succeed after the fix. (Our local workaround added the `/`; temporarily
remove it to test.)

## 5. Upstream contribution process

1. Search existing issues first: [#2190](https://github.com/espressif/arduino-esp32/issues/2190)
   (closed, partial fix) is the anchor; also skim newer issues for duplicates
   mentioning "DNS Failed" + query string (e.g. [#9712](https://github.com/espressif/arduino-esp32/issues/9712)
   looks related). If no open duplicate: open a new issue referencing #2190,
   stating the `?`/`#` cases were never covered. An issue first is the polite
   route and gives the PR something to link.
2. Fork `espressif/arduino-esp32`, branch from `master`
   (e.g. `fix/httpclient-host-terminator`).
3. Read `CONTRIBUTING.rst` in the repo root. The project uses **pre-commit**
   hooks (clang-format etc.) — install and run them or CI will flag style.
4. Espressif requires a **CLA** — the bot will prompt on the first PR; sign it.
5. PR against `master` with: the table from §4 in the description, the curl
   normalization demo as evidence of expected behavior, and `Closes #<issue>`.
6. Patch is small and self-contained — good first-PR material. Expect requests
   to also check `HTTPUpdate`/`WebServer` don't depend on the old behavior
   (grep for `beginInternal` callers; there are only the `begin()` overloads).

## 6. Background links

- RFC 3986 §3.2 (authority component): https://datatracker.ietf.org/doc/html/rfc3986#section-3.2
- Original partial fix discussion: https://github.com/espressif/arduino-esp32/issues/2190
- ESP-IDF's `esp_http_client` is NOT affected (uses `http_parser_parse_url`,
  which is spec-compliant) — useful as a correctness reference.
- Local workaround already shipped in this repo: `src/main.cpp` uses
  `https://api.ipify.org/?format=json` (explicit `/`). Keep the workaround even
  after the upstream fix lands — PlatformIO's packaged core lags upstream by
  months.
