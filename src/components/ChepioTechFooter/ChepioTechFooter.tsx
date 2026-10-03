// Core
import type { MouseEvent } from "react";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Styles
import "./ChepioTechFooter.css";
// Consts
import messages from "@/messages/messages";

const CHEPIO_URL = "https://chepio.tech";

/** Mandatory developer credit (chepio-footer skill). Must stay the last element of the app footer. */
function ChepioTechFooter() {
  // A webview cannot open new windows; hand the allow-listed URL to the system browser instead.
  const openInBrowser = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    void fileforgeApi.openExternal(CHEPIO_URL);
  };

  return (
    <div className="chepio-footer">
      <div className="chepio-footer__inner">
        <a
          href={CHEPIO_URL}
          target="_blank"
          rel="noopener noreferrer"
          className="chepio-footer__link"
          aria-label={messages.footer.credit}
          onClick={openInBrowser}
        >
          <img src="/images/chepio-tech/logo_designed.svg" alt="chepio.tech" className="chepio-footer__logo" />
        </a>
      </div>
    </div>
  );
}

export default ChepioTechFooter;
