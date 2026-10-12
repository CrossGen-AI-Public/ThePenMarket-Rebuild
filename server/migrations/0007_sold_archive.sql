-- The Sold Pen Archive and deleting pens (Nathaniel, meeting of 2026-10-05).
--
-- sold_at: when a pen became sold, whether by the admin status menu or a Stripe sale. Kept by a trigger so no code path can forget it; cleared when a pen goes back on sale.
-- Pens already sold before this migration have no known sale date and stay NULL (sorted after dated ones).
--
-- A "deleted" pen is status 'archived': off the website and out of every list, its row kept for his
-- accounts until he erases it for good from /admin/deleted-pens/.

ALTER TABLE product ADD COLUMN sold_at TIMESTAMPTZ;
CREATE INDEX product_sold_at_idx ON product (sold_at DESC) WHERE status = 'sold';

CREATE FUNCTION product_sold_at() RETURNS trigger AS $$
BEGIN
    -- A pen imported as already sold has no known sale date; only a change to sold is dated.
    IF TG_OP = 'UPDATE' AND NEW.status = 'sold' AND OLD.status IS DISTINCT FROM 'sold' THEN
        NEW.sold_at := now();
    ELSIF NEW.status IN ('live', 'draft', 'reserved') THEN
        NEW.sold_at := NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER product_sold_at BEFORE INSERT OR UPDATE OF status ON product
    FOR EACH ROW EXECUTE FUNCTION product_sold_at();

-- When he deleted it, so the Deleted pens list reads newest first.
ALTER TABLE product ADD COLUMN deleted_at TIMESTAMPTZ;

-- The description and photo the buyer saw, copied onto the order when it is paid, so the receipt and
-- the order record keep them even if the listing is edited or erased later.
ALTER TABLE shop_order ADD COLUMN product_description TEXT NOT NULL DEFAULT '';
ALTER TABLE shop_order ADD COLUMN product_image TEXT NOT NULL DEFAULT '';
