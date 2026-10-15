# ADR-0001: The site never links to a page that no longer exists

> Decision date: 2026-10-12
> Decided by: Brittany ("don't point to links that don't exist anymore")
> Status: Accepted (implemented on `feat-launch-ready`)
> Scope: links to other websites inside posts, custom pages, product descriptions and Trading Post ads; `server/src/links.rs`, migration 0008, `/admin/links/`
> Related: the CLAUDE.md rule "FAQ answers, custom pages and posts are his words"; `routes/legacy.rs` (our own old addresses, the other half of "no dead ends")

## Context

The full-site audit of 2026-10-12 followed every link on all 707 pages in the sitemaps, 42,642 in all. After the old ASP.NET and WordPress addresses were redirected, no link to our own site was broken. Six links to other websites, all inside Nathaniel's blog posts from 2013 to 2021, were:

| Address | What it answered |
|---|---|
| `https://www.dallaspenshow.com/wp/` | 404 |
| `http://www.arkansaspenclub.com/2018-pen-show.html` | 404 |
| `http://www.iampeth.com/` | 404 |
| `http://www.nancybach.com` | nothing (no server) |
| `https://www.facebook.com/NancyBachAuthor/` | 400 (Facebook refuses robots) |
| `https://www.usatoday.com/story/news/factcheck/2020/08/13/…` | 402 (paywall / robot block) |

The first four are pages or whole websites that are gone. The last two very likely still exist and only turn away an automated check.

Pages on other websites keep disappearing (pen shows end, clubs let domains lapse), so a one-time fix would rot. Posts are Nathaniel's words, and nothing on the site is written or changed on his behalf. Editing the stored posts to remove the links would break that rule. It would also be permanent, even for a page that is only down for a while.

## Decision

1. **A dead outside link is shown as its own words, not as a link.** When a page is rendered, an `<a>` whose address is confirmed dead becomes `<span class="link-gone">` around the same text. The visitor reads what Nathaniel wrote and is not sent to a dead end. The stored HTML is never modified.
2. **"Dead" means gone twice, at least 20 hours apart.** A daily check (`links::check_all`, first run five minutes after start, then every 24 hours) requests every outside link in published posts, custom pages, live or sold products and live Trading Post ads. HEAD comes first, then GET, with a browser-like user agent and a 20-second limit.
   - **Gone:** 404, 410, a host that no longer exists, or nothing listening.
   - **Unsure:** 401, 402, 403, 429, any 5xx, a timeout, or a failed handshake. An unsure link **stays linked**, because a page that exists must not lose its link just because its site dislikes robots.
   - **Ok:** 2xx or 3xx. An ok answer clears any run of "gone", so a link that comes back is linked again on the next check, by itself.
3. **Only the public internet is checked.** A link whose host resolves to a private, loopback, link-local or Tailscale address is skipped and stays as it is, so content can never make the server probe its own network.
4. **Nathaniel can see every decision.** `/admin/links/` (a tile on the admin home with a count) lists three groups: links no longer shown as links, with the page each is on and since when; links that were gone once and will be checked again; and links that would not answer the check. A button runs the check now. To point a dead link somewhere new, he edits the post; nothing else is needed.
5. **Links leaving published content are forgotten.** `external_link` holds only addresses that still appear in something published.
6. **The audit agrees.** `scripts/full_audit.sh` fails on any broken link to our own site. Outside links that fail are written to `links-external.txt` for review and do not fail the audit, because the site itself now handles them.

## Alternatives Considered

- **Edit the posts and remove the links (what was first proposed).** It is permanent, it changes his words, and it does nothing about the next link to die. Rejected.
- **Point dead links at the Internet Archive's copy (`web.archive.org/web/…`).** It keeps the reference alive and is common practice. But it sends visitors off to a third party that may have no copy or the wrong copy, and it adds a dependency on archive.org's availability. Possible later as an opt-in per link; not the default.
- **Mark dead links instead of unlinking them** (struck through, or "link no longer works"). That adds wording to his posts that he did not write. Rejected for now; the `link-gone` class leaves room for styling if he wants it.
- **Treat 403, 402 and 429 as dead too.** That would have unlinked Facebook and USA Today, which almost certainly still work for people. Rejected: false positives are worse than a rare dead link that a robot can't detect.
- **Check once, at deploy.** Links die between deploys. Rejected in favour of the daily check.

## Consequences

Positive
- No visitor follows a link from the site to a page that is confirmed gone, now or later, with no work from Nathaniel.
- His posts are untouched, and a link that comes back is restored automatically.
- He has one admin page showing exactly which links were turned off and why.

Negative
- A site that answers 404 to robots but not to people would be unlinked wrongly. It would show on `/admin/links/`; there is no per-link "keep it linked" override yet.
- A link dies up to about two days before it is unlinked (two checks, 20 hours apart).
- The server makes one outbound request per outside link per day (a few hundred, 300 ms apart). `LINK_CHECK=0` turns the check off, for example on a machine with no internet access.
- Links whose sites only refuse robots stay linked even if they are in fact gone. They are listed on `/admin/links/` for a person to judge.
