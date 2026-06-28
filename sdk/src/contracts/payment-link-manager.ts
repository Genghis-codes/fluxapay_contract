import { NetworkProfileSwitcher, NetworkEnvironment } from "../network-profiles.js";
import { withMappedContractError } from "../index.js";

export interface PaymentLinkManagerConfig {
  network: NetworkEnvironment;
  rpcUrl?: string;
  contractId: string;
}

/**
 * Represents a payment link stored on-chain in the PaymentLinkManager contract.
 */
export interface PaymentLink {
  /** Unique link identifier */
  link_id: string;
  /** Stellar address of the merchant that created the link */
  merchant: string;
  /** Optional fixed amount in stroops; undefined means the payer supplies the amount */
  amount?: bigint;
  /** Whether this link is currently active */
  active: boolean;
  /** USDC token contract address */
  usdc_token: string;
  /** Arbitrary key/value metadata attached to the link */
  metadata?: Record<string, string>;
}

/**
 * Parameters for creating a new payment link.
 */
export interface CreateLinkParams {
  /** The merchant's Stellar address */
  merchant: string;
  /** Optional fixed amount in stroops */
  amount?: bigint;
  /** USDC token contract address */
  usdcToken: string;
  /** Arbitrary metadata (e.g. product info, order reference) */
  metadata?: Record<string, string>;
}

/**
 * PaymentLinkManagerClient provides a high-level interface for interacting
 * with the PaymentLinkManager Soroban contract.
 */
export class PaymentLinkManagerClient {
  private contract: any;
  public networkSwitcher: NetworkProfileSwitcher;
  private contractId: string;
  private rpcUrl: string;
  private networkPassphrase: string;

  constructor(config: PaymentLinkManagerConfig) {
    this.networkSwitcher = new NetworkProfileSwitcher(config.network);
    const profile = this.networkSwitcher.getProfile();
    this.rpcUrl = config.rpcUrl || profile.rpcUrl;
    this.networkPassphrase = profile.networkPassphrase;
    this.contractId = config.contractId;
  }

  private getContract(): any {
    if (!this.contract) {
      const { Client } = require("@stellar/stellar-sdk/contract");
      this.contract = new Client({
        networkPassphrase: this.networkPassphrase,
        rpcUrl: this.rpcUrl,
        contractId: this.contractId,
      });
    }
    return this.contract;
  }

  /**
   * Switch the client to a different network environment.
   * @param environment - Target network environment
   * @param contractId - Optional new contract ID
   */
  public switchNetwork(environment: NetworkEnvironment, contractId?: string): void {
    this.networkSwitcher.switchEnvironment(environment);
    const profile = this.networkSwitcher.getProfile();
    this.rpcUrl = profile.rpcUrl;
    this.networkPassphrase = profile.networkPassphrase;
    if (contractId) {
      this.contractId = contractId;
    }
    this.contract = undefined;
  }

  /**
   * Create a new payment link.
   * @param params - Link creation parameters
   * @returns A promise resolving to the new link ID
   */
  async createLink(params: CreateLinkParams): Promise<string> {
    return withMappedContractError(() =>
      this.getContract().create_link({
        merchant: params.merchant,
        amount: params.amount,
        usdc_token: params.usdcToken,
        metadata: params.metadata,
      }),
    );
  }

  /**
   * Use a payment link to initiate a payment.
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
    return withMappedContractError(() =>
      this.getContract().use_link({
        payer,
        link_id: linkId,
        amount,
        usdc_token: usdcToken,
      }),
    );
  }

  /**
   * Deactivate a payment link (merchant only).
   * @param merchant - The merchant's Stellar address
   * @param linkId - The payment link ID to deactivate
   */
  async deactivateLink(merchant: string, linkId: string): Promise<void> {
    return withMappedContractError(() =>
      this.getContract().deactivate_link({
        merchant,
        link_id: linkId,
      }),
    );
  }

  /**
   * Retrieve details of a specific payment link.
   * @param linkId - The payment link ID
   * @returns A promise resolving to the PaymentLink details
   */
  async getLink(linkId: string): Promise<PaymentLink> {
    return withMappedContractError(() =>
      this.getContract().get_link({
        link_id: linkId,
      }),
    );
  }

  /**
   * Verify a batch of payment links, returning only the still-active ones.
   * @param linkIds - Array of link IDs to verify
   * @returns A promise resolving to an array of active link IDs
   */
  async verifyBatch(linkIds: string[]): Promise<string[]> {
    return withMappedContractError(() =>
      this.getContract().verify_batch({
        link_ids: linkIds,
      }),
    );
  }
}
