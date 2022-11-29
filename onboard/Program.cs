﻿using System;
using System.Reflection;
using log4net;
using onboard.devcade;

namespace onboard
{
    public static class Program
    {
        [STAThread]
        private static void Main() {
            // Logging setup
            GlobalContext.Properties["LogFilePath"] = "/tmp/devcade/logs";
            GlobalContext.Properties["LogFileName"] = ".log";
            log4net.Config.XmlConfigurator.Configure();
            LogManager.GetLogger(MethodBase.GetCurrentMethod()?.DeclaringType?.FullName).Info("Starting application");

            // Set namespace log levels
            LogConfig.init();

            // Application setup
            Client.start();
            
            using var game = new ui.Devcade();
            game.Run();

            // using var game = new Game1();
            // game.Run();
        }
    }
}
