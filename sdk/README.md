# @fluxapay/sdk

Official TypeScript SDK for interacting with FluxaPay's Soroban smart contracts on the Stellar network.

## Installation

```bash
npm install @fluxapay/sdk
```

## Release Notes

See [CHANGELOG.md](./CHANGELOG.md) for version history.

## Quick Start

```typescript
import { FluxapayClient } from "@fluxapay/sdk";

const client = new FluxapayClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...", // PaymentProcessor contract ID
  merchantRegistryContractId: "C...", // MerchantRegistry contract ID (optional)
});

async function main() {
  // Create a payment with full CreatePaymentArgs support
  const payment = await client.createPayment({
    paymentId: "pay_123",
    merchantId: "G...",
    amount: 1000000n, // 1 USDC
    currency: "USDC",
    depositAddress: "G...",
    expiresAt: BigInt(Math.floor(Date.now() / 1000) + 3600),
    durationSecs: 3600n,           // optional: alternative to expiresAt
    memo: "Order #42",             // optional
    memoType: "Text",              // optional: Text | Id | Hash | Return
    tokenAddress: "C...",          // optional: custom token
    clientToken: "idempotency-key", // optional: idempotency key
  });

  console.log("Payment created:", payment);

  // Get payment status
  const status = await client.getPayment("pay_123");
  console.log("Payment status:", status);
}
```

## Features

- **High-level Wrapper**: `FluxapayClient`, `RefundManagerClient`, `MerchantRegistryClient`, and `FxOracleClient` simplify complex contract interactions.
- **Typed Interfaces**: Full TypeScript support for all contract models (`Merchant`, `PaymentCharge`, `Refund`, `FeeConfig`, etc.).
- **Automatic Simulation**: Built-in support for Soroban transaction simulation.
- **Network Presets**: Easy switching between `testnet` and `mainnet`.

## Merchant Management (FluxapayClient)

Register and manage merchants directly through `FluxapayClient`. Pass `merchantRegistryContractId` in config to target the dedicated MerchantRegistry contract.

### Register without custom fee

```typescript
await client.registerMerchant({
  merchantId: "G...",
  businessName: "Acme Corp",
  settlementCurrency: "USDC",
  payoutAddress: "G...",
});
```

### Register with custom FeeConfig

```typescript
import { FluxapayClient, FeeConfig } from "@fluxapay/sdk";

const feeConfig: FeeConfig = {
  platform_fee_bps: 200n,   // 2%
  fixed_fee: 100000n,       // 0.01 USDC fixed fee
  fee_recipient: "G...",    // optional custom recipient
};

await client.registerMerchant({
  merchantId: "G...",
  businessName: "Acme Corp",
  settlementCurrency: "USDC",
  payoutAddress: "G...",
  feeConfig,
});
```

### Update, verify, and query merchants

```typescript
// Update merchant settings (including fee config)
await client.updateMerchant({
  merchantId: "G...",
  businessName: "Updated Corp Name",
  settlementCurrency: "EUR",
  feeConfig: {
    platform_fee_bps: 150n,
    fixed_fee: 0n,
    fee_recipient: undefined,
  },
});

// Verify merchant (admin only)
await client.verifyMerchant("G...", "G..."); // admin, merchantId

// Get merchant details
const merchant = await client.getMerchant("G...");
console.log("Merchant:", merchant);
```

## Refunds and Disputes (FluxapayClient)

```typescript
// Create a refund request
const refundTx = await client.createRefund({
  paymentId: "pay_123",
  amount: 500000n,
  reason: "Damaged goods",
  requester: "G...",
});

// Process a pending refund (operator)
await client.processRefund("G...", "refund_001");

// Query refunds
const refund = await client.getRefund("refund_001");
const paymentRefunds = await client.getPaymentRefunds("pay_123");

// Create a dispute
const disputeTx = await client.createDispute({
  paymentId: "pay_123",
  amount: 500000n,
  reason: "Unauthorized charge",
  evidence: "ipfs://...",
  disputer: "G...",
});

// Dispute lifecycle (operator)
await client.reviewDispute("G...", "dispute_001");
await client.resolveDisputeWithRefund("G...", "dispute_001", "Refund approved");
// or: await client.rejectDispute("G...", "dispute_001", "Insufficient evidence");

// Query disputes
const dispute = await client.getDispute("dispute_001");
const paymentDisputes = await client.getPaymentDisputes("pay_123");
```

## RefundManagerClient

The `RefundManagerClient` provides methods for managing refunds on a dedicated RefundManager contract:

```typescript
import { RefundManagerClient } from "@fluxapay/sdk";

const refundClient = new RefundManagerClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...", // RefundManager contract ID
});

async function handleRefund() {
  const refundId = await refundClient.createRefund(
    "payment_123",
    500000n,
    "Damaged goods",
    "G...",
  );

  const refund = await refundClient.getRefund(refundId);
  await refundClient.processRefund("G...", refundId);
  const allRefunds = await refundClient.getPaymentRefunds("payment_123");
}
```

## MerchantRegistryClient

The standalone `MerchantRegistryClient` is also available for direct registry access:

```typescript
import { MerchantRegistryClient, FeeConfig } from "@fluxapay/sdk";

const merchantClient = new MerchantRegistryClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...",
});

// Without fee config
await merchantClient.registerMerchant({
  merchantId: "merchant_001",
  businessName: "Acme Corp",
  settlementCurrency: "USDC",
});

// With fee config
const feeConfig: FeeConfig = {
  platform_fee_bps: 100n,
  fixed_fee: 50000n,
  fee_recipient: undefined,
};

await merchantClient.registerMerchant({
  merchantId: "merchant_002",
  businessName: "Beta Inc",
  settlementCurrency: "USDC",
  feeConfig,
});

await merchantClient.verifyMerchant("G...", "merchant_001");
await merchantClient.updateMerchant({
  merchantId: "merchant_001",
  businessName: "Updated Corp Name",
});
```

## FxOracleClient

The `FxOracleClient` provides methods for querying and publishing FX exchange rates.

### Standalone client

```typescript
import { FxOracleClient } from "@fluxapay/sdk";

const oracleClient = new FxOracleClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  oracleContractId: "C...",
});

const rate = await oracleClient.getRate("USDCNGN");
const settlementAmount = await oracleClient.getSettlementAmount(1_000_000n, "NGN");
```

### Via FluxapayClient

```typescript
const client = new FluxapayClient({
  network: "testnet",
  contractId: "C...",
  oracleContractId: "C...",
});

const oracle = client.fxOracle();
const rate = await oracle.getRate("USDCNGN");
```

## Payment Links (FluxapayClient)

Payment links let merchants share a reusable URL that payers can settle against. Pass `paymentLinkContractId` in config to enable these methods.

### Create a payment link

```typescript
import { FluxapayClient } from "@fluxapay/sdk";

const client = new FluxapayClient({
  network: "testnet",
  contractId: "C...",
  paymentLinkContractId: "C...", // PaymentLinkManager contract ID
});

// Fixed-amount link
const linkId = await client.createLink({
  merchant: "G...",
  amount: 5_000_000n, // 0.5 USDC (7 decimals)
  usdcToken: "C...",
  metadata: { product: "Coffee", ref: "order_42" }, // optional
});
console.log("Link created:", linkId);

// Open-amount link (payer sets the amount)
const openLinkId = await client.createLink({
  merchant: "G...",
  usdcToken: "C...",
});
```

### Use a payment link

```typescript
await client.useLink(
  "G...",        // payer address
  linkId,        // link ID returned by createLink
  5_000_000n,    // amount in stroops
  "C...",        // USDC token contract address
);
```

### Retrieve and verify links

```typescript
// Fetch a single link
const link = await client.getLink(linkId);
console.log("Link active:", link.active);
console.log("Merchant:", link.merchant);
console.log("Metadata:", link.metadata);

// Batch-verify multiple links (returns only active link IDs)
const activeLinkIds = await client.verifyBatch([linkId, openLinkId, "C_other..."]);
console.log("Active links:", activeLinkIds);
```

### Deactivate a link

```typescript
// Only the merchant that created the link can deactivate it
await client.deactivateLink("G...", linkId);
```

### Standalone PaymentLinkManagerClient

```typescript
import { PaymentLinkManagerClient } from "@fluxapay/sdk";

const linkClient = new PaymentLinkManagerClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...", // PaymentLinkManager contract ID
});

const linkId = await linkClient.createLink({
  merchant: "G...",
  amount: 10_000_000n,
  usdcToken: "C...",
  metadata: { item: "Widget" },
});

const link = await linkClient.getLink(linkId);
await linkClient.useLink("G_payer...", linkId, 10_000_000n, "C...");
await linkClient.deactivateLink("G_merchant...", linkId);
const active = await linkClient.verifyBatch([linkId]);
```

## License

MIT

## Publishing

Releases are published to npm when a version tag is pushed:

```bash
git tag sdk/v0.1.0
git push origin sdk/v0.1.0
```

The [SDK Release](https://github.com/MetroLogic/fluxapay_contract/actions/workflows/sdk-release.yml) workflow builds, tests, and publishes `@fluxapay/sdk`. Requires `NPM_TOKEN` in GitHub repository secrets (npm automation token with publish access to the `@fluxapay` scope).
