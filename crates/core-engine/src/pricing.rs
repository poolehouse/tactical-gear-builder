//! Build cost & competitive vendor pricing.
//!
//! All prices are integer US cents (`u32`) — no floats anywhere, so BOM
//! rollups are exact and byte-deterministic (R5). Best-offer selection is
//! fully deterministic: among in-stock offers the lowest price wins, ties
//! broken by vendor name in lexicographic order. When no in-stock offer
//! exists, the effective price falls back to MSRP and savings are zero —
//! a fabricated price is never invented (R2 spirit).

use serde::{Deserialize, Serialize};

/// A colorway a product is offered in, or a build is configured as.
///
/// Regulation policies constrain colorways by this typed enum — never by
/// parsing product names (R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colorway {
    /// MultiCam.
    Multicam,
    /// Coyote Brown.
    CoyoteBrown,
    /// Ranger Green.
    RangerGreen,
    /// Wolf Grey.
    WolfGrey,
    /// Black.
    Black,
    /// Woodland M81.
    WoodlandM81,
}

/// One competitive vendor listing for a catalog item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorOffer {
    /// Display name of the vendor (e.g. `"Optactical"`).
    pub vendor_name: String,
    /// Offered price in US cents.
    pub price_cents: u32,
    /// Whether the vendor currently reports stock for this listing.
    pub in_stock: bool,
    /// Product page URL. Display only — the browser demo never fetches it.
    pub product_url: String,
}

/// Deterministically select the best in-stock offer: lowest price wins,
/// ties broken by vendor name in lexicographic order.
///
/// Out-of-stock listings are ignored entirely; `None` means no in-stock
/// offer exists (the caller falls back to MSRP).
pub fn best_offer(offers: &[VendorOffer]) -> Option<&VendorOffer> {
    offers
        .iter()
        .filter(|offer| offer.in_stock)
        .min_by(|a, b| (a.price_cents, &a.vendor_name).cmp(&(b.price_cents, &b.vendor_name)))
}

/// The effective unit price: the best in-stock offer, falling back to MSRP
/// when no vendor holds stock.
pub fn effective_price_cents(msrp_cents: u32, offers: &[VendorOffer]) -> u32 {
    best_offer(offers).map_or(msrp_cents, |offer| offer.price_cents)
}

/// MSRP minus the effective unit price, clamped at zero.
pub fn savings_cents(msrp_cents: u32, offers: &[VendorOffer]) -> u32 {
    msrp_cents.saturating_sub(effective_price_cents(msrp_cents, offers))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(vendor: &str, price: u32, in_stock: bool) -> VendorOffer {
        VendorOffer {
            vendor_name: vendor.to_owned(),
            price_cents: price,
            in_stock,
            product_url: format!("https://example.com/{vendor}"),
        }
    }

    #[test]
    fn best_offer_picks_lowest_in_stock_price() {
        let offers = [
            offer("SKD Tactical", 14_499, true),
            offer("Optactical", 13_999, true),
            offer("Manufacturer Direct", 12_999, false), // cheaper, but OOS
        ];
        assert_eq!(best_offer(&offers).unwrap().vendor_name, "Optactical");
    }

    #[test]
    fn best_offer_breaks_price_ties_by_vendor_name() {
        let offers = [
            offer("SKD Tactical", 13_999, true),
            offer("Optactical", 13_999, true),
        ];
        assert_eq!(best_offer(&offers).unwrap().vendor_name, "Optactical");
    }

    #[test]
    fn no_in_stock_offers_falls_back_to_msrp() {
        let offers = [offer("SKD Tactical", 9_999, false)];
        assert!(best_offer(&offers).is_none());
        assert_eq!(effective_price_cents(14_999, &offers), 14_999);
        assert_eq!(savings_cents(14_999, &offers), 0);
    }

    #[test]
    fn savings_never_go_negative() {
        // An in-stock offer priced above MSRP must not fabricate negative
        // savings.
        let offers = [offer("Primary Arms", 16_999, true)];
        assert_eq!(effective_price_cents(14_999, &offers), 16_999);
        assert_eq!(savings_cents(14_999, &offers), 0);
    }
}
