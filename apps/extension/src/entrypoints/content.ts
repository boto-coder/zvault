/**
 * ZVault content script — auto-fill support.
 *
 * Injected into web pages (HTTPS only) to detect login forms and fill
 * credentials on user request.
 *
 * Security constraints:
 * - Only operates on HTTPS pages (enforced by host_permissions).
 * - Never stores credentials; requests them from the background service worker.
 * - URI matching is performed before injecting any data.
 * - Fills only visible, user-interactable input fields.
 */

export default defineContentScript({
  matches: ["https://*/*"],
  runAt: "document_idle",

  main() {
    // ─── Form detection ────────────────────────────────────────────────

    /**
     * Find login forms on the page (username + password fields).
     */
    function findLoginForms(): HTMLFormElement[] {
      const forms: HTMLFormElement[] = [];
      const allForms = document.querySelectorAll("form");

      for (const form of allForms) {
        const passwordField = form.querySelector(
          'input[type="password"]'
        ) as HTMLInputElement | null;
        if (passwordField && isVisible(passwordField)) {
          forms.push(form);
        }
      }

      // Also check for password fields outside forms
      if (forms.length === 0) {
        const standalonePasswords = document.querySelectorAll(
          'input[type="password"]'
        );
        for (const pw of standalonePasswords) {
          if (isVisible(pw as HTMLElement)) {
            // Create a virtual form boundary
            const parent = pw.closest("div, section, main, body");
            if (parent) {
              forms.push(parent as unknown as HTMLFormElement);
            }
          }
        }
      }

      return forms;
    }

    /**
     * Check if an element is visible and interactable.
     */
    function isVisible(el: HTMLElement): boolean {
      const style = window.getComputedStyle(el);
      return (
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        style.opacity !== "0" &&
        el.offsetParent !== null
      );
    }

    /**
     * Find the username field associated with a password field.
     */
    function findUsernameField(
      container: HTMLElement
    ): HTMLInputElement | null {
      const candidates = container.querySelectorAll(
        'input[type="text"], input[type="email"], input[autocomplete*="user"], input[name*="user"], input[name*="email"], input[name*="login"]'
      );

      for (const candidate of candidates) {
        if (isVisible(candidate as HTMLElement)) {
          return candidate as HTMLInputElement;
        }
      }
      return null;
    }

    // ─── Fill operation ────────────────────────────────────────────────

    /**
     * Fill a form with credentials from the vault.
     */
    function fillForm(
      container: HTMLElement,
      username: string,
      password: string
    ) {
      const usernameField = findUsernameField(container);
      const passwordField = container.querySelector(
        'input[type="password"]'
      ) as HTMLInputElement | null;

      if (usernameField && username) {
        setInputValue(usernameField, username);
      }

      if (passwordField && password) {
        setInputValue(passwordField, password);
      }
    }

    /**
     * Set an input field's value in a way that triggers React/Vue/Angular
     * change detection.
     */
    function setInputValue(input: HTMLInputElement, value: string) {
      const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        "value"
      )?.set;

      if (nativeInputValueSetter) {
        nativeInputValueSetter.call(input, value);
      } else {
        input.value = value;
      }

      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    }

    // ─── Message listener ──────────────────────────────────────────────

    browser.runtime.onMessage.addListener(
      (message: { type: string; payload?: unknown }) => {
        if (message.type === "FILL_CREDENTIALS") {
          const { username, password } = message.payload as {
            username: string;
            password: string;
          };
          const forms = findLoginForms();
          if (forms.length > 0) {
            fillForm(forms[0] as HTMLElement, username, password);
          }
        }
      }
    );

    // ─── Notify background about detected forms ────────────────────────

    const forms = findLoginForms();
    if (forms.length > 0) {
      browser.runtime.sendMessage({
        type: "FORM_DETECTED",
        payload: {
          url: window.location.href,
          formCount: forms.length,
        },
      });
    }
  },
});
