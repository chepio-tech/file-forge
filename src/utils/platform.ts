/** Marks the root element with the OS so CSS can account for platform chrome (macOS overlay title bar). */
export function markPlatform(root: HTMLElement = document.documentElement, userAgent = navigator.userAgent): void {
  if (/Macintosh|Mac OS X/.test(userAgent)) root.classList.add("platform-mac");
}

export default markPlatform;
