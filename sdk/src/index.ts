import {
  Client as ContractClient,
  Merchant,
  PaymentCharge,
  Refund,
  Dispute,
  PaymentStatus,
  RefundStatus,
  DisputeStatus,
  FeeConfig,
  MaybeFeeConfig,
  CreatePaymentArgs,
} from "./contracts/fluxapay/src/index.js";
import { Networks } from "@stellar/stellar-sdk";
import {
  FluxapayOfflineSigner,
  OfflineTransactionPayload,
  buildOfflinePayload,
  buildCreatePaymentPayload,
  buildVerifyPaymentPayload,
  buildCreateRefundPayload,
  prepareForOfflineSigning,
  restoreFromOfflinePayload,
} from "./offline-signer.js";
import { NetworkProfileSwitcher, NetworkEnvironment, NetworkProfiles, NetworkProfile } from "./network-profiles.js";
import { FxOracleClient } from "./contracts/fx-oracle.js";
import { MerchantRegistryClient } from "./contracts/merchant-registry.js";
import {
  PaymentLinkManagerClient,
  type PaymentLinkManagerConfig,
  type PaymentLink,
  type CreateLinkParams,
} from "./contracts/payment-link-manager.js";

export interface FluxapayConfig {
  network: NetworkEnvironment;
  rpcUrl?: string;
  contractId: string;
  /** FX Oracle contract ID for multi-currency rate queries. */
  oracleContractId?: string;
  /** MerchantRegistry contract ID for merchant management operations. */
  merchantRegistryContractId?: string;
  /** PaymentLinkManager contract ID for payment link operations. */
  paymentLinkContractId?: string;
}

export interface CreatePaymentParams {
  paymentId: string;
  merchantId: string;
  amount: bigint;
  currency: string;
  depositAddress: string;
  expiresAt?: bigint;
  durationSecs?: bigint;
  memo?: string;
  memoType?: string;
  tokenAddress?: string;
  clientToken?: string;
}

export interface RegisterMerchantParams {
  merchantId: string;
  businessName: string;
  settlementCurrency: string;
  payoutAddress?: string;
  bankAccount?: string;
  feeConfig?: FeeConfig;
}

export interface UpdateMerchantParams {
  merchantId: string;
  businessName?: string;
  settlementCurrency?: string;
  active?: boolean;
  payoutAddress?: string;
  bankAccount?: string;
  feeConfig?: FeeConfig;
}

export const FLUXAPAY_CONTRACT_ERROR_MAP: Record<number, string> = {
  1: "Unauthorized",
  2: "PaymentAlreadyExists",
  3: "PaymentExpired",
  4: "InvalidPaymentId",
  8: "RefundAlreadyProcessed",
  9: "DisputeNotFound",
  12: "DisputeAlreadyResolved",
  14: "PaymentAlreadyProcessed",
  15: "AccessControlError",
  16: "RefundExceedsPayment",
  17: "ContractPaused",
  18: "RateLimitExceeded",
  19: "RefundCancelled",
  20: "UnsupportedToken",
  21: "AmountBelowMin",
  22: "AmountAboveMax",
  23: "InvalidExpiry",
  24: "InvalidSettlement",
  25: "DuplicateIdempotencyKey",
  26: "InvalidAddress",
  27: "ArbitrageDetected",
  28: "SwapPathInvalid",
  29: "OraclePriceDeviation",
  30: "SubscriptionInGracePeriod",
  31: "SubscriptionRetryExhausted",
  32: "InvalidResumeTimestamp",
  33: "MerchantAuthError",
  34: "TierVolumeLimitExceeded",
  35: "RefundExpired",
  36: "InsufficientArbitrators",
  37: "ArbitrationVotingThresholdNotMet",
  38: "FeeProposalNotReady",
  39: "NoFeeProposal",
  404: "PaymentNotFound",
  405: "RefundNotFound",
  406: "InvalidAmount",
};

export class FluxapayError extends Error {
  readonly code: number;
  readonly contractErrorName: string;
  readonly cause?: unknown;

  constructor(code: number, contractErrorName: string, message?: string, cause?: unknown) {
    super(message ?? contractErrorName);
    this.name = `${contractErrorName}Error`;
    this.code = code;
    this.contractErrorName = contractErrorName;
    this.cause = cause;
  }
}

const HOST_ERROR_CODE_REGEX = /Error\(Contract,\s*#(\d+)\)/;

function parseContractErrorCode(error: unknown): number | null {
  if (typeof error !== "object" || error === null) {
    return null;
  }

  const maybeCode = (error as { code?: unknown }).code;
  if (typeof maybeCode === "number") {
    return maybeCode;
  }

  const maybeMessage = (error as { message?: unknown }).message;
  if (typeof maybeMessage === "string") {
    const match = maybeMessage.match(HOST_ERROR_CODE_REGEX);
    if (match && match[1]) {
      return Number(match[1]);
    }
  }

  const maybeResult = (error as { result?: unknown }).result;
  if (typeof maybeResult === "string") {
    const match = maybeResult.match(HOST_ERROR_CODE_REGEX);
    if (match && match[1]) {
      return Number(match[1]);
    }
  }

  return null;
}

function toFluxapayError(error: unknown): FluxapayError {
  const code = parseContractErrorCode(error);
  if (code === null) {
    if (error instanceof Error) {
      throw error;
    }
    throw new Error("Unknown Fluxapay SDK error");
  }

  const contractErrorName = FLUXAPAY_CONTRACT_ERROR_MAP[code] ?? "UnknownContractError";
  return new FluxapayError(
    code,
    contractErrorName,
    `${contractErrorName} (contract error #${code})`,
    error,
  );
}

async function withMappedContractError<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw toFluxapayError(error);
  }
}

function toCreatePaymentArgs(params: CreatePaymentParams): CreatePaymentArgs {
  return {
    payment_id: params.paymentId,
    merchant_id: params.merchantId,
    amount: params.amount,
    currency: params.currency,
    deposit_address: params.depositAddress,
    expires_at: params.expiresAt,
    duration_secs: params.durationSecs,
    memo: params.memo,
    memo_type: params.memoType,
    token_address: params.tokenAddress,
    client_token: params.clientToken,
    metadata_hash: undefined,
    metadata: undefined,
  };
}

export class FluxapayClient {
  public contract: ContractClient;
  public networkSwitcher: NetworkProfileSwitcher;
  private fxOracleClient?: FxOracleClient;
  private merchantRegistryClient?: MerchantRegistryClient;
  private paymentLinkManagerClient?: PaymentLinkManagerClient;
  private readonly config: FluxapayConfig;

  constructor(config: FluxapayConfig) {
    this.config = config;
    this.networkSwitcher = new NetworkProfileSwitcher(config.network);

    const rpcUrl = config.rpcUrl || this.networkSwitcher.getProfile().rpcUrl;

    this.contract = new ContractClient({
      networkPassphrase: this.networkSwitcher.getProfile().networkPassphrase,
      rpcUrl: rpcUrl,
      contractId: config.contractId,
    });
  }

  private getMerchantRegistry(): MerchantRegistryClient {
    if (!this.config.merchantRegistryContractId) {
      throw new Error(
        "merchantRegistryContractId is required in FluxapayConfig for merchant registry operations",
      );
    }

    if (!this.merchantRegistryClient) {
      const profile = this.networkSwitcher.getProfile();
      this.merchantRegistryClient = new MerchantRegistryClient({
        network: profile.environment,
        rpcUrl: this.config.rpcUrl || profile.rpcUrl,
        contractId: this.config.merchantRegistryContractId,
      });
    }

    return this.merchantRegistryClient;
  }

  /**
   * Get an FX Oracle client when `oracleContractId` was provided in config.
   */
  fxOracle(): FxOracleClient {
    if (!this.config.oracleContractId) {
      throw new Error(
        "oracleContractId is required in FluxapayConfig to use the FX Oracle client",
      );
    }

    if (!this.fxOracleClient) {
      const profile = this.networkSwitcher.getProfile();
      this.fxOracleClient = new FxOracleClient({
        network: profile.environment,
        rpcUrl: this.config.rpcUrl || profile.rpcUrl,
        oracleContractId: this.config.oracleContractId,
      });
    }

    return this.fxOracleClient;
  }

  /**
   * Switch the client to a different network environment.
   * This re-initializes the contract client seamlessly.
   */
  public switchNetwork(environment: NetworkEnvironment, contractId?: string): void {
    this.networkSwitcher.switchEnvironment(environment);
    const profile = this.networkSwitcher.getProfile();
    const newContractId = contractId || profile.defaultContractId || this.contract.options.contractId;

    this.contract = new ContractClient({
      networkPassphrase: profile.networkPassphrase,
      rpcUrl: profile.rpcUrl,
      contractId: newContractId,
    });
    this.fxOracleClient = undefined;
    this.merchantRegistryClient = undefined;
    this.paymentLinkManagerClient = undefined;
  }

  /**
   * Create a new payment charge
   */
  async createPayment(params: CreatePaymentParams) {
    return withMappedContractError(() =>
      this.contract.create_payment(toCreatePaymentArgs(params)),
    );
  }

  /**
   * Verify a payment via oracle
   */
  async verifyPayment(params: {
    oracle: string;
    paymentId: string;
    transactionHash: Buffer;
    payerAddress: string;
    amountReceived: bigint;
  }) {
    return withMappedContractError(() =>
      this.contract.verify_payment({
        oracle: params.oracle,
        payment_id: params.paymentId,
        transaction_hash: params.transactionHash,
        payer_address: params.payerAddress,
        amount_received: params.amountReceived,
      }),
    );
  }

  /**
   * Register a new merchant in the MerchantRegistry contract
   */
  async registerMerchant(params: RegisterMerchantParams) {
    if (this.config.merchantRegistryContractId) {
      return this.getMerchantRegistry().registerMerchant(params);
    }

    return withMappedContractError(() =>
      this.contract.register_merchant({
        merchant_id: params.merchantId,
        business_name: params.businessName,
        settlement_currency: params.settlementCurrency,
        payout_address: params.payoutAddress,
        bank_account: params.bankAccount,
        fee_config: params.feeConfig,
      }),
    );
  }

  /**
   * Update merchant settings in the MerchantRegistry contract
   */
  async updateMerchant(params: UpdateMerchantParams) {
    if (this.config.merchantRegistryContractId) {
      return this.getMerchantRegistry().updateMerchant(params);
    }

    return withMappedContractError(() =>
      this.contract.update_merchant({
        merchant_id: params.merchantId,
        business_name: params.businessName,
        settlement_currency: params.settlementCurrency,
        active: params.active,
        payout_address: params.payoutAddress,
        bank_account: params.bankAccount,
        fee_config: params.feeConfig,
      }),
    );
  }

  /**
   * Get merchant details
   */
  async getMerchant(merchantId: string) {
    if (this.config.merchantRegistryContractId) {
      return this.getMerchantRegistry().getMerchant(merchantId);
    }

    return withMappedContractError(() =>
      this.contract.get_merchant({
        merchant_id: merchantId,
      }),
    );
  }

  /**
   * Verify a merchant (admin only)
   */
  async verifyMerchant(admin: string, merchantId: string) {
    if (this.config.merchantRegistryContractId) {
      return this.getMerchantRegistry().verifyMerchant(admin, merchantId);
    }

    return withMappedContractError(() =>
      this.contract.verify_merchant({
        admin,
        merchant_id: merchantId,
      }),
    );
  }

  /**
   * Create a refund request
   */
  async createRefund(params: {
    paymentId: string;
    amount: bigint;
    reason: string;
    requester: string;
  }) {
    return withMappedContractError(() =>
      this.contract.create_refund({
        payment_id: params.paymentId,
        refund_amount: params.amount,
        reason: params.reason,
        requester: params.requester,
      }),
    );
  }

  /**
   * Process a pending refund
   */
  async processRefund(operator: string, refundId: string) {
    return withMappedContractError(() =>
      this.contract.process_refund({
        operator,
        refund_id: refundId,
      }),
    );
  }

  /**
   * Get refund details by ID
   */
  async getRefund(refundId: string) {
    return withMappedContractError(() =>
      this.contract.get_refund({
        refund_id: refundId,
      }),
    );
  }

  /**
   * Get all refunds for a payment
   */
  async getPaymentRefunds(paymentId: string) {
    return withMappedContractError(() =>
      this.contract.get_payment_refunds({
        payment_id: paymentId,
      }),
    );
  }

  /**
   * Create a dispute for a payment
   */
  async createDispute(params: {
    paymentId: string;
    amount: bigint;
    reason: string;
    evidence: string;
    disputer: string;
  }) {
    return withMappedContractError(() =>
      this.contract.create_dispute({
        payment_id: params.paymentId,
        amount: params.amount,
        reason: params.reason,
        evidence: params.evidence,
        disputer: params.disputer,
      }),
    );
  }

  /**
   * Move a dispute to under-review status
   */
  async reviewDispute(operator: string, disputeId: string) {
    return withMappedContractError(() =>
      this.contract.review_dispute({
        operator,
        dispute_id: disputeId,
      }),
    );
  }

  /**
   * Resolve a dispute by issuing a refund
   */
  async resolveDisputeWithRefund(
    operator: string,
    disputeId: string,
    notes: string,
  ) {
    return withMappedContractError(() =>
      this.contract.resolve_dispute_with_refund({
        operator,
        dispute_id: disputeId,
        resolution_notes: notes,
      }),
    );
  }

  /**
   * Reject a dispute
   */
  async rejectDispute(operator: string, disputeId: string, notes: string) {
    return withMappedContractError(() =>
      this.contract.reject_dispute({
        operator,
        dispute_id: disputeId,
        resolution_notes: notes,
      }),
    );
  }

  /**
   * Get dispute details by ID
   */
  async getDispute(disputeId: string) {
    return withMappedContractError(() =>
      this.contract.get_dispute({
        dispute_id: disputeId,
      }),
    );
  }

  /**
   * Get all disputes for a payment
   */
  async getPaymentDisputes(paymentId: string) {
    return withMappedContractError(() =>
      this.contract.get_payment_disputes({
        payment_id: paymentId,
      }),
    );
  }

  /**
   * Get payment details
   */
  async getPayment(paymentId: string) {
    return withMappedContractError(() =>
      this.contract.get_payment({ payment_id: paymentId }),
    );
  }

  private getPaymentLinkManager(): PaymentLinkManagerClient {
    if (!this.config.paymentLinkContractId) {
      throw new Error(
        "paymentLinkContractId is required in FluxapayConfig for payment link operations",
      );
    }

    if (!this.paymentLinkManagerClient) {
      const profile = this.networkSwitcher.getProfile();
      this.paymentLinkManagerClient = new PaymentLinkManagerClient({
        network: profile.environment,
        rpcUrl: this.config.rpcUrl || profile.rpcUrl,
        contractId: this.config.paymentLinkContractId,
      });
    }

    return this.paymentLinkManagerClient;
  }

  /**
   * Create a new payment link.
   * Maps to `PaymentLinkManager.create_link` on-chain.
   * @param params.merchant - The merchant's Stellar address
   * @param params.amount - Optional fixed amount in stroops
   * @param params.usdcToken - USDC token contract address
   * @param params.metadata - Optional key/value metadata
   * @returns A promise resolving to the new link ID
   */
  async createLink(params: CreateLinkParams): Promise<string> {
    return this.getPaymentLinkManager().createLink(params);
  }

  /**
   * Use a payment link to initiate a payment.
   * Maps to `PaymentLinkManager.use_link` on-chain.
   * @param payer - The payer's Stellar address
   * @param linkId - The payment link ID
   * @param amount - The amount to pay in stroops
   * @param usdcToken - The USDC token contract address
   */
  async useLink(
    payer: string,
    linkId: string,
    amount: bigint,
    usdcToken: string,
  ): Promise<void> {
    return this.getPaymentLinkManager().useLink(payer, linkId, amount, usdcToken);
  }

  /**
   * Deactivate a payment link (merchant only).
   * Maps to `PaymentLinkManager.deactivate_link` on-chain.
   * @param merchant - The merchant's Stellar address
   * @param linkId - The payment link ID to deactivate
   */
  async deactivateLink(merchant: string, linkId: string): Promise<void> {
    return this.getPaymentLinkManager().deactivateLink(merchant, linkId);
  }

  /**
   * Retrieve details of a specific payment link.
   * Maps to `PaymentLinkManager.get_link` on-chain.
   * @param linkId - The payment link ID
   * @returns A promise resolving to the PaymentLink details
   */
  async getLink(linkId: string): Promise<PaymentLink> {
    return this.getPaymentLinkManager().getLink(linkId);
  }

  /**
   * Verify a batch of payment links, returning only active ones.
   * Maps to `PaymentLinkManager.verify_batch` on-chain.
   * @param linkIds - Array of link IDs to verify
   * @returns A promise resolving to an array of active link IDs
   */
  async verifyBatch(linkIds: string[]): Promise<string[]> {
    return this.getPaymentLinkManager().verifyBatch(linkIds);
  }

  /** Offline/hardware wallet payload builder utilities. */
  offlineSigner(): FluxapayOfflineSigner {
    return new FluxapayOfflineSigner(
      this.contract as import("./offline-signer.js").OfflineCapableClient,
      this.contract.options.contractId,
      this.contract.options.networkPassphrase,
    );
  }
}

export { toFluxapayError, withMappedContractError };

export {
  Merchant,
  PaymentCharge,
  Refund,
  Dispute,
  PaymentStatus,
  RefundStatus,
  DisputeStatus,
  FeeConfig,
  MaybeFeeConfig,
  CreatePaymentArgs,
  FluxapayOfflineSigner,
  OfflineTransactionPayload,
  buildOfflinePayload,
  buildCreatePaymentPayload,
  buildVerifyPaymentPayload,
  buildCreateRefundPayload,
  prepareForOfflineSigning,
  restoreFromOfflinePayload,
  NetworkProfileSwitcher,
  NetworkEnvironment,
  NetworkProfiles,
  NetworkProfile,
};

export { RefundManagerClient, type RefundManagerConfig } from "./contracts/refund-manager.js";
export { MerchantRegistryClient, type MerchantRegistryConfig } from "./contracts/merchant-registry.js";
export {
  FxOracleClient,
  FxOracleError,
  type FxOracleConfig,
  type RateData,
  FX_ORACLE_ERROR_MAP,
} from "./contracts/fx-oracle.js";
export {
  PaymentLinkManagerClient,
  type PaymentLinkManagerConfig,
  type PaymentLink,
  type CreateLinkParams,
} from "./contracts/payment-link-manager.js";
