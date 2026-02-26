import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import Sidebar from "@/components/Sidebar";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Prodz - Productivity Tracker",
  description: "A beautiful Pomodoro productivity timer and tracker.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="dark">
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased bg-white text-black min-h-screen flex flex-col md:flex-row overflow-hidden font-sans`}
      >
          <Sidebar />

        <main className="flex-grow flex flex-col h-screen overflow-y-auto bg-white p-4 md:p-8">
          <div className="max-w-6xl w-full mx-auto pb-20">
            {children}
          </div>
        </main>
      </body>
    </html>
  );
}
