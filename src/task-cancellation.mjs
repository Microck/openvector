/** Shared task signal. Keep this owner independent of the numerical document:
 * profile selection also runs in the browser, without Node bitmap codecs. */
export class CancelTaskException extends Error {
  constructor(message) {super(message);this.name = 'CancelTaskException';}
}
