import { expect, test as base } from "@playwright/test";
import { FixtureProvider } from "./fixture-provider";
import { clearTestCredentials, launchNativeApp, type NativeApp } from "./native-app";

/**
 * One loopback provider and one native app per test. The provider already
 * accepts the suite's initial throwaway key; tests add or remove candidates
 * as the scenario needs. Cleanup always kills the process, removes the
 * isolated application-data tree, and deletes the run's keyring namespace.
 */
export const test = base.extend<{ provider: FixtureProvider; app: NativeApp }>({
  provider: async ({}, use) => {
    const provider = new FixtureProvider();
    provider.acceptedKeys.add("sk-native-old");
    await provider.start();
    await use(provider);
    await provider.stop();
  },
  app: async ({ provider: _provider }, use) => {
    const app = await launchNativeApp();
    try {
      await use(app);
    } finally {
      await app.close();
      await clearTestCredentials(app.dirs.credentialService);
    }
  },
});

export { expect };

export type { NativeApp };
