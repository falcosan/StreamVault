const SITE_PAGE = "/robots.txt";

export const refererMissing = () => location.protocol === "file:";

export async function fetchFromSite(url, seconds) {
  const signal = AbortSignal.timeout(seconds * 1000);
  const frame = document.createElement("iframe");
  frame.hidden = true;
  frame.sandbox = "allow-same-origin";
  frame.src = new URL(SITE_PAGE, url).href;
  try {
    await new Promise((resolve, reject) => {
      frame.onload = resolve;
      signal.onabort = () => reject(signal.reason);
      document.body.append(frame);
    });
    const response = await frame.contentWindow.fetch(url, { signal });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return await response.text();
  } catch (error) {
    throw String(error?.message ?? error);
  } finally {
    frame.remove();
  }
}
