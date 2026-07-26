import { NetworkProfileSwitcher, NetworkEnvironment } from "../network-profiles.js";

export interface GasEstimatorConfig {
  network: NetworkEnvironment;
  rpcUrl?: string;
  gasEstimatorContractId: string;
}

/** Mirrors the on-chain `Operation` enum in `gas_estimator.rs`. */
export type GasOperation =
  | "CreatePayment"
  | "VerifyPayment"
  | "CancelPayment"
  | "ExpirePayment"
  | "SettlePayment"
  | "CreateRefund"
  | "ProcessRefund"
  | "RejectRefund"
  | "CancelRefund"
  | "CreateDispute"
  | "ResolveDispute"
  | "RejectDispute"
  | "SwapAndPay"
  | "CreateStream"
  | "WithdrawStream"
  | "CancelStream";

/** Mirrors the on-chain `CostEstimate` struct returned by `GasEstimator`. */
export interface GasEstimate {
  operation: GasOperation;
  instructions: bigint;
  ledgerReads: number;
  ledgerWrites: number;
  events: number;
  resourceFeeStroops: bigint;
}

function fromContractEstimate(raw: {
  operation: GasOperation;
  instructions: bigint;
  ledger_reads: number;
  ledger_writes: number;
  events: number;
  resource_fee_stroops: bigint;
}): GasEstimate {
  return {
    operation: raw.operation,
    instructions: raw.instructions,
    ledgerReads: raw.ledger_reads,
    ledgerWrites: raw.ledger_writes,
    events: raw.events,
    resourceFeeStroops: raw.resource_fee_stroops,
  };
}

/**
 * GasEstimatorClient provides a high-level interface for querying on-chain
 * Soroban resource cost estimates from the `GasEstimator` contract before
 * submitting a transaction.
 */
export class GasEstimatorClient {
  private contract: any;
  public networkSwitcher: NetworkProfileSwitcher;
  private gasEstimatorContractId: string;
  private rpcUrl: string;
  private networkPassphrase: string;

  constructor(config: GasEstimatorConfig) {
    this.networkSwitcher = new NetworkProfileSwitcher(config.network);
    const profile = this.networkSwitcher.getProfile();
    this.rpcUrl = config.rpcUrl || profile.rpcUrl;
    this.networkPassphrase = profile.networkPassphrase;
    this.gasEstimatorContractId = config.gasEstimatorContractId;
  }

  private getContract(): any {
    if (!this.contract) {
      const { Client } = require("@stellar/stellar-sdk/contract");
      this.contract = new Client({
        networkPassphrase: this.networkPassphrase,
        rpcUrl: this.rpcUrl,
        contractId: this.gasEstimatorContractId,
      });
    }
    return this.contract;
  }

  /**
   * Switch the client to a different network environment.
   * @param environment - The target network environment (e.g., 'testnet', 'mainnet')
   * @param gasEstimatorContractId - Optional GasEstimator contract ID for the new network
   */
  public switchNetwork(environment: NetworkEnvironment, gasEstimatorContractId?: string): void {
    this.networkSwitcher.switchEnvironment(environment);
    const profile = this.networkSwitcher.getProfile();
    this.rpcUrl = profile.rpcUrl;
    this.networkPassphrase = profile.networkPassphrase;
    if (gasEstimatorContractId) {
      this.gasEstimatorContractId = gasEstimatorContractId;
    }
    this.contract = undefined;
  }

  /** Estimate the resource cost of a single operation. */
  async estimate(operation: GasOperation): Promise<GasEstimate> {
    const raw = await this.getContract().estimate({ op: operation });
    return fromContractEstimate(raw);
  }

  /** Estimate the resource cost of every supported operation. */
  async estimateAll(): Promise<GasEstimate[]> {
    const raw: Array<Parameters<typeof fromContractEstimate>[0]> =
      await this.getContract().estimate_all();
    return raw.map(fromContractEstimate);
  }

  /** Fetch the on-chain Symbol name for an operation (useful for display). */
  async operationName(operation: GasOperation): Promise<string> {
    return this.getContract().operation_name({ op: operation });
  }
}
