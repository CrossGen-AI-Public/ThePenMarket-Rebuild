-- tradingpost@thepenmarket.com no longer exists (Nathaniel's e-mail of 2026-10-12): where his FAQ answers
-- and pages give it as the address to write to, they now give info@, the one he reads. Only the
-- address changes; every other word is his.
UPDATE faq SET answer = replace(answer, 'tradingpost@thepenmarket.com', 'info@thepenmarket.com'), updated_at = now() WHERE answer ILIKE '%tradingpost@thepenmarket.com%';
UPDATE page SET body_html = replace(replace(body_html, 'mailto:tradingpost@thepenmarket.com', 'mailto:info@thepenmarket.com'), 'tradingpost@thepenmarket.com', 'info@thepenmarket.com') WHERE body_html ILIKE '%tradingpost@thepenmarket.com%';
