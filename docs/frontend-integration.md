# Frontend Integration Notes

This document is a quick index for frontend teams integrating RemitWise contracts.

## Contract Design Docs

- [Family Wallet Design (as implemented)](family-wallet-design.md)

## Family Wallet UI Integration Checklist

Use the family wallet design doc above as the source of truth for behavior. For frontend implementation, make sure to support:

- Role-aware UI actions (`Owner`, `Admin`, `Member`, `Viewer`)
- Pending multisig lifecycle (`tx_id > 0`) vs immediate execution (`tx_id == 0`)
- Emergency mode UX and guardrails (`max_amount`, `cooldown`, `min_balance`)
- Pause-state handling (`is_paused`) for mutation paths
- Transaction expiry handling and cleanup (`cleanup_expired_pending`)
- Event-driven updates for member changes, emergency transfers, and wallet admin state
