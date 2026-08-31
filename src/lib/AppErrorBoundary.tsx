import { Component, type ErrorInfo, type ReactNode } from "react";
import { captureAppError } from "./sentryCapture";

type AppErrorBoundaryProps = {
  children: ReactNode;
  fallback: ReactNode;
};

type AppErrorBoundaryState = {
  failed: boolean;
};

/** Keeps React crash containment synchronous while reporting lazily to Sentry. */
export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): AppErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, _info: ErrorInfo) {
    captureAppError(error, "react_error_boundary");
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}
