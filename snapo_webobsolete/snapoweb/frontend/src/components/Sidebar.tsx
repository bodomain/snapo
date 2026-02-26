"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { Timer, BarChart2, Database, LayoutDashboard } from "lucide-react";

export default function Sidebar() {
  const pathname = usePathname();

  const navItems = [
    { name: "Dashboard", href: "/", icon: LayoutDashboard },
    { name: "Settings", href: "/settings", icon: Database },
  ];

  return (
    <aside className="w-full md:w-64 bg-white border-r border-gray-200 p-6 flex flex-col gap-8 h-auto md:h-screen shrink-0">
      <div className="flex items-center gap-3">
        <div className="w-10 h-10 rounded-xl bg-black flex items-center justify-center shadow-lg shadow-gray-200/20">
          <Timer className="w-5 h-5 text-white" />
        </div>
        <h1 className="text-xl font-bold text-black">
          Prodz
        </h1>
      </div>

      <nav className="flex flex-col gap-2 flex-grow">
        {navItems.map((item) => {
          const isActive = pathname === item.href;
          const Icon = item.icon;
          return (
            <Link
              key={item.name}
              href={item.href}
              className={`flex items-center gap-3 px-4 py-3 rounded-xl font-medium transition-all duration-200 ${
                isActive
                  ? "bg-black/10 text-black shadow-sm border border-gray-200/20"
                  : "text-gray-600 hover:text-gray-800 hover:bg-gray-100 border border-transparent"
              }`}
            >
              <Icon className="w-5 h-5" />
              <span>{item.name}</span>
            </Link>
          );
        })}
        <div className="flex items-center gap-3 px-4 py-3 rounded-xl text-gray-400 font-medium transition-colors hover:text-gray-600 hover:bg-gray-100 border border-transparent cursor-not-allowed opacity-50 mt-1">
          <BarChart2 className="w-5 h-5" />
          <span>Analytics (Soon)</span>
        </div>
      </nav>

      <div className="mt-auto px-4 py-3 bg-gray-100 rounded-xl text-xs text-gray-500 text-center border border-gray-200/50 shadow-inner">
        Prodz Web v1.0
      </div>
    </aside>
  );
}
