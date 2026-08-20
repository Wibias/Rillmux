import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export function CopyableDeviceCode({ code }: { code: string }) {
  const { t } = useTranslation("common");
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  async function copy() {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 1800);
  }

  return (
    <span className="authbar__code-wrap">
      <button
        type="button"
        className="authbar__code"
        onClick={() => void copy()}
        aria-label={t("copyCode", { code })}
      >
        {code}
      </button>
      {copied ? (
        <span className="authbar__copied" role="status">
          {t("codeCopied")}
        </span>
      ) : null}
    </span>
  );
}
