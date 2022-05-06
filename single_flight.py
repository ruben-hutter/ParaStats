import numpy as np
from datetime import datetime


class SingleFlight:
    def __init__(self, file):
        # vario data
        self.flight_name = ""
        self.vario_data = []
        self.sens_alt = []
        self.gps_alt = []
        self.sens_gps_mean = []
        self.alt_diff = []
        self.takeoff_time = 0
        self.landing_time = 0
        self.flight_duration = 0
        # initialize object
        self.initialize_flight(file)

    def initialize_flight(self, file):
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
        self.takeoff_time = self.convert_time(self.vario_data[0][0])
        self.landing_time = self.convert_time(
            self.vario_data[self.vario_data.shape[0] - 1][0]
        )
        self.flight_duration = self.landing_time - self.takeoff_time

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
                    global flight_name
                    flight_name = line[10:]

    # convert time to correct format
    def convert_time(self, time):
        time = str(time)
        if len(time) < 6:
            time = "0" + time
        time = datetime.strptime(
            ":".join(time[i : i + 2] for i in range(0, len(time), 2)), "%H:%M:%S"
        )
        return time

    def print_flight_stats(self):
        divider = "".join(["=" for _ in range(50)])
        print(divider)
        print(f"Flight name: {flight_name}")
        print(f"Flight duration:\t{self.flight_duration}")
        print(f"Takeoff altitude:\t{self.sens_gps_mean[0]} m")
        print(f"Landing altitude:\t{self.sens_gps_mean[-1]} m")
        print(f"Max altitude:\t\t{np.max(self.sens_gps_mean)} m")
        print(f"Max climb:\t\t{np.max(self.alt_diff)} m/s")
        print(f"Max sink:\t\t{np.min(self.alt_diff)} m/s")
        print(divider)

    def extract_altitudes(self):
        for n in range(self.vario_data.shape[0] - 1):
            self.sens_alt.append(self.vario_data[n][3])
            self.gps_alt.append(self.vario_data[n][4])
            # calculate mean alt
            self.calculate_mean_alt(np.mean([self.sens_alt[-1], self.gps_alt[-1]]))

    def calculate_mean_alt(self, mean_val):
        self.sens_gps_mean.append(mean_val)
