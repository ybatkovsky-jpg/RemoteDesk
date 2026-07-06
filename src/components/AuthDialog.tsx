import { createSignal } from "solid-js";

interface AuthDialogProps {
  mode: "host" | "client";
  onConfirm: (password: string) => void;
  onCancel: () => void;
}

export default function AuthDialog(props: AuthDialogProps) {
  const [password, setPassword] = createSignal("");
  const [showPwd, setShowPwd] = createSignal(false);

  const handleSubmit = (e: Event) => {
    e.preventDefault();
    if (password().trim()) {
      props.onConfirm(password().trim());
    }
  };

  return (
    <div class="auth-overlay" onClick={(e) => e.target === e.currentTarget && props.onCancel()}>
      <div class="auth-modal">
        <h3>
          {props.mode === "host" ? "🔒 Set Host Password" : "🔑 Enter Password"}
        </h3>
        <p class="hint">
          {props.mode === "host"
            ? "Clients will need this password to connect."
            : "Enter the host's password to connect."}
        </p>
        <form onSubmit={handleSubmit}>
          <div class="auth-input-row">
            <input
              type={showPwd() ? "text" : "password"}
              value={password()}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Password"
              autofocus
            />
            <button
              type="button"
              class="toggle-pwd"
              onClick={() => setShowPwd(!showPwd())}
            >
              {showPwd() ? "🙈" : "👁"}
            </button>
          </div>
          <div class="auth-buttons">
            <button type="button" onClick={props.onCancel}>
              {props.mode === "host" ? "Skip" : "Cancel"}
            </button>
            <button type="submit" class="primary" disabled={!password().trim()}>
              {props.mode === "host" ? "Set Password" : "Connect"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
