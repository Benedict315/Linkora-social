import React from "react";
import { Logo } from "@/components/branding/Logo";

interface HeaderProps {
  className?: string;
  children?: React.ReactNode;
}

export const Header: React.FC<HeaderProps> = ({ className = "", children }) => {
  return (
    <header className={`flex items-center justify-between px-4 py-3 border-b border-[var(--border)] bg-[var(--background)]/80 backdrop-blur-sm ${className}`}>
      <div className="flex items-center gap-3">
        <Logo variant="full" size="md" />
      </div>
      {children && <div className="flex items-center gap-3">{children}</div>}
    </header>
  );
};

export default Header;
