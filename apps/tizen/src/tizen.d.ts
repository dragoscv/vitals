/**
 * The slice of Samsung's Tizen Web Device API this app touches.
 *
 * Hand-written because Samsung publishes no npm types, and only the calls
 * actually made are described — a full copy would be a second, unchecked
 * spec. Everything is optional at runtime: the `tizen` global does not exist
 * in a desktop browser, where the app runs for development and tests.
 */

interface TizenSystemInfo {
  getPropertyValue(
    property: string,
    success: (value: unknown) => void,
    error?: (error: { name: string; message: string }) => void,
  ): void;
  getCapability(key: string): unknown;
  getTotalMemory(): number;
  getAvailableMemory(): number;
}

interface TizenApplication {
  getCurrentApplication(): {
    exit(): void;
    appInfo: { version: string; id: string };
  };
}

interface TizenTvInputDevice {
  registerKeyBatch(
    keys: string[],
    success?: () => void,
    error?: (error: { name: string; message: string }) => void,
  ): void;
}

interface TizenGlobal {
  systeminfo?: TizenSystemInfo;
  application?: TizenApplication;
  tvinputdevice?: TizenTvInputDevice;
}

declare const tizen: TizenGlobal | undefined;

/** The package version, from `package.json`, set by Vite's `define`. */
declare const __APP_VERSION__: string;
