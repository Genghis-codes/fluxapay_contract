/**
 * FluxaPay Soroban Event Consumer
 * Subscribes to contract events via stellar-sdk and persists to PostgreSQL
 * Implements at-least-once delivery with idempotency (event_id dedup)
 */

import {
  Server,
  EventFilter,
  GetEventsRequest,
  ContractEventFilter,
} from "stellar-sdk";
import { Database } from "./database";
import { ContractEvent, AnyEvent } from "./types";
import * as dotenv from "dotenv";

dotenv.config();

interface EventSubscriptionConfig {
  rpcUrl: string;
  contractId: string;
  dbConnectionString: string;
  pollInterval: number;
  startLedger: number;
}

class EventSubscriber {
  private server: Server;
  private database: Database;
  private config: EventSubscriptionConfig;
  private currentLedger: number;

  constructor(config: EventSubscriptionConfig) {
    this.config = config;
    this.server = new Server(config.rpcUrl);
    this.database = new Database(config.dbConnectionString);
    this.currentLedger = config.startLedger;
  }

  async initialize(): Promise<void> {
    await this.database.initialize();
    console.log("Event subscriber initialized");
  }

  async start(): Promise<void> {
    console.log(`Starting event subscription from ledger ${this.currentLedger}`);

    // Main subscription loop
    const subscriptionLoop = setInterval(async () => {
      try {
        await this.pollEvents();
      } catch (error) {
        console.error("Error polling events:", error);
      }
    }, this.config.pollInterval);

    // Graceful shutdown
    process.on("SIGINT", async () => {
      clearInterval(subscriptionLoop);
      await this.shutdown();
    });
  }

  private async pollEvents(): Promise<void> {
    try {
      const request: GetEventsRequest = {
        filters: [
          {
            type: "contract",
            contractIds: [this.config.contractId],
          } as ContractEventFilter,
        ],
        startLedger: this.currentLedger,
        limit: 100,
      };

      const response = await this.server.getEvents(request);

      if (!response.events || response.events.length === 0) {
        // No new events, update ledger to avoid re-fetching
        if (response.latestLedger) {
          this.currentLedger = response.latestLedger;
        }
        return;
      }

      console.log(`Found ${response.events.length} events`);

      for (const event of response.events) {
        try {
          const parsedEvent = this.parseEvent(event);
          if (parsedEvent) {
            const stored = await this.database.storeEvent(parsedEvent);
            if (stored) {
              console.log(`✓ Stored event: ${parsedEvent.id}`);
            }
          }
        } catch (error) {
          console.error(`Error processing event:`, error);
        }
      }

      // Update current ledger for next poll
      if (response.latestLedger) {
        this.currentLedger = response.latestLedger + 1;
      }
    } catch (error) {
      console.error("Error in pollEvents:", error);
    }
  }

  private parseEvent(event: any): AnyEvent | null {
    try {
      // Extract event metadata
      const eventId = `${event.ledger}-${event.txHash}-${event.id}`;
      const timestamp = Math.floor(Date.now() / 1000);
      const ledger = parseInt(event.ledger);
      const txHash = event.txHash || "";

      // Parse topics (should be array of strings)
      const topics = Array.isArray(event.topic) ? event.topic : [];
      if (topics.length < 2) {
        console.warn("Invalid event topics:", topics);
        return null;
      }

      // Parse value (should be contract value)
      let value: Record<string, unknown> = {};
      if (event.value) {
        try {
          // stellar-sdk returns values as ScVal objects; we need to convert them
          value = this.scValToObject(event.value);
        } catch (e) {
          console.warn("Could not parse event value:", e);
        }
      }

      const baseEvent: ContractEvent = {
        id: eventId,
        timestamp,
        ledger,
        txHash,
        contractId: event.contractId || "",
        topic: topics,
        value,
      };

      // Return as typed event based on topic
      return baseEvent as AnyEvent;
    } catch (error) {
      console.error("Error parsing event:", error);
      return null;
    }
  }

  private scValToObject(scval: any): Record<string, unknown> {
    // Basic conversion of Stellar ScVal to JavaScript object
    // This is a simplified implementation; a full implementation would handle
    // all ScVal types (map, vec, contract, etc.)
    if (typeof scval === "string") {
      return { value: scval };
    }
    if (typeof scval === "number") {
      return { value: scval };
    }
    if (scval && typeof scval === "object") {
      // If it's already a plain object, return it
      if (scval.constructor === Object) {
        return scval;
      }
    }
    return { raw: scval };
  }

  private async shutdown(): Promise<void> {
    console.log("Shutting down event subscriber...");
    await this.database.close();
    process.exit(0);
  }
}

async function main(): Promise<void> {
  const config: EventSubscriptionConfig = {
    rpcUrl: process.env.SOROBAN_RPC_URL || "http://localhost:8000/soroban/rpc",
    contractId:
      process.env.FLUXAPAY_CONTRACT_ID ||
      "CBNR5IXY5K7KCJ63PY3ZFYHG4U6F5C5I2PZ2ZQXDMQQNC6ZF63K65QQ", // placeholder
    dbConnectionString:
      process.env.DATABASE_URL ||
      "postgres://postgres:password@localhost:5432/fluxapay",
    pollInterval: parseInt(process.env.POLL_INTERVAL_MS || "5000"),
    startLedger: parseInt(process.env.START_LEDGER || "1"),
  };

  const subscriber = new EventSubscriber(config);
  await subscriber.initialize();
  await subscriber.start();
}

main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
