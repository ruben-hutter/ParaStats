from datetime import datetime

import numpy as np

from constants import Constants


def convert_time(time):
    time = str(time)
    if len(time) < 6:
        time = "0" + time
    time = datetime.strptime(
        ":".join(time[i: i + 2] for i in range(0, len(time), 2)), "%H:%M:%S"
    )
    return time


class SingleFlight(Constants):
    def __init__(self, file):
        # vario data
        self.flight_name = ""
        self.flight_date = ""
        self.vario_data = []
        self.sens_alt = []
        self.gps_alt = []
        self.sens_gps_mean = []
        self.max_altitude = 0
        self.alt_diff = []
        self.alt_diff_max = 0
        self.alt_diff_min = 0
        self.integrated_alt_diff = []
        self.max_integrated_climb = 0
        self.max_integrated_sink = 0
        self.takeoff_time = 0
        self.landing_time = 0
        self.flight_duration = 0
        # initialize obj
        self.initialize_flight(file)

    def initialize_flight(self, file):
        # parse vario data
        self.parse_vario_data(file)
        # array to np.array
        self.vario_data = np.array(self.vario_data)
        # extract altitudes and calculate mean
        self.extract_altitudes()
        # arrays to np.arrays
        self.sens_alt = np.array(self.sens_alt)
        self.gps_alt = np.array(self.gps_alt)
        self.sens_gps_mean = np.array(self.sens_gps_mean)
        # calculate diff of altitude in m/s
        self.alt_diff = np.array(np.diff(self.sens_gps_mean))
        # calculate flight duration
        self.takeoff_time = convert_time(self.vario_data[0][0])
        self.landing_time = convert_time(
            self.vario_data[self.vario_data.shape[0] - 1][0]
        )
        self.flight_duration = self.landing_time - self.takeoff_time

        self.calculate_integrated_alt_diff()
        self.calculate_max_and_min_alt_diff()

    def parse_vario_data(self, file):
        # open file
        with open(file) as data:
            # read lines starting with B, and append altitude to lists
            while True:
                line = data.readline()
                # end of file
                if not line:
                    return
                if line.startswith("B"):
                    # B 094957 4607012N 00853636E A 01495 01540
                    # divide line into:
                    # (time, longitude, latitude, sens alt, gps alt)
                    line = (
                        int(line[1:7]),
                        int(line[7:14]),
                        int(line[15:23]),
                        int(line[25:30]),
                        int(line[30:-1]),
                    )
                    self.vario_data.append(line)
                elif line.startswith("HFSITSITE"):
                    self.flight_name = line[10:]
                elif line.startswith("HFDTE"):
                    date = line[5:-1]
                    self.flight_date = "/".join(
                        date[i: i + 2] for i in range(0, len(date), 2)
                    )

    def print_stats(self):
        print(Constants.PRINTING_DIVIDER)
        print(f"Flight name:\t{self.flight_name}")
        print(f"Date:\t{self.flight_date}")
        print(f"Takeoff:\t{self.takeoff_time.time()}")
        print(f"Landing:\t{self.landing_time.time()}")
        print(f"Flight duration:\t{self.flight_duration}")
        print(f"Takeoff altitude:\t{self.sens_gps_mean[0]} m")
        print(f"Landing altitude:\t{self.sens_gps_mean[-1]} m")
        print(f"Max altitude:\t{self.max_altitude} m")
        print(f"Max climb rate:\t{self.alt_diff_max} m/s")
        print(f"Max sink rate:\t{self.alt_diff_min} m/s")
        print(
            f"Max integrated climb ({Constants.INTEGRATION_TIME} s):\t{self.max_integrated_climb:.1f} m/s"
        )
        print(
            f"Max integrated sink ({Constants.INTEGRATION_TIME} s):\t{self.max_integrated_sink:.1f} m/s"
        )
        print(Constants.PRINTING_DIVIDER)

    def extract_altitudes(self):
        for n in range(self.vario_data.shape[0]):
            self.sens_alt.append(self.vario_data[n][3])
            self.gps_alt.append(self.vario_data[n][4])
            # calculate mean alt
            self.calculate_mean_alt(np.mean([self.sens_alt[-1], self.gps_alt[-1]]))

    def calculate_mean_alt(self, mean_val):
        self.sens_gps_mean.append(mean_val)

    def calculate_integrated_alt_diff(self):
        self.integrated_alt_diff = np.array(
            [
                (
                        self.sens_gps_mean[n + Constants.INTEGRATION_TIME - 1]
                        - self.sens_gps_mean[n]
                )
                / Constants.INTEGRATION_TIME
                for n in range(
                0, self.sens_gps_mean.shape[0] - Constants.INTEGRATION_TIME
            )
            ]
        )

    def calculate_max_and_min_alt_diff(self):
        self.max_integrated_climb = np.max(self.integrated_alt_diff)
        self.max_integrated_sink = np.min(self.integrated_alt_diff)
        self.max_altitude = np.max(self.sens_gps_mean)
        self.alt_diff_max = np.max(self.alt_diff)
        self.alt_diff_min = np.min(self.alt_diff)
