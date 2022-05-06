from curses.ascii import alt
import numpy as np
import sys
from datetime import datetime

# vario data
flight_name = ""
vario_data = []
sens_alt = []
gps_alt = []
sens_gps_mean = []
alt_diff = []
flight_duration = 0

def parse_vario_data(file):
	# open file
	with open(file) as data:
		# read lines starting with B, and append altitude to lists
		while True:
			line = data.readline()
			# end of file
			if not line:
				return
			if line.startswith('B'):
                # B 094957 4607012N 00853636E A 01495 01540
                # divide line into: (time, longitude, latitude, sens alt, gps alt)
				line = (int(line[1:7]), int(line[7:14]), int(line[15:23]), int(line[25:30]), int(line[30:-1]))
				vario_data.append(line)
			elif line.startswith("HFSITSITE"):
				global flight_name
				flight_name = line[10:]

# convert time to correct format
def convert_time(time):
	time = str(time)
	if len(time) < 6:
		time = '0' + time
	time = datetime.strptime(":".join(time[i:i+2] for i in range(0, len(time), 2)), "%H:%M:%S")
	return time

def print_flight_stats():
	divider = ''.join(['=' for _ in range(50)])
	print(divider)
	print(f"Flight name: {flight_name}")
	print(f"Flight duration:\t{flight_duration}")
	print(f"Takeoff altitude:\t{sens_gps_mean[0]} m")
	print(f"Landing altitude:\t{sens_gps_mean[-1]} m")
	print(f"Max altitude:\t\t{np.max(sens_gps_mean)} m")
	print(f"Max climb:\t\t{np.max(alt_diff)} m/s")
	print(f"Max sink:\t\t{np.min(alt_diff)} m/s")
	print(divider)

def main():
	global vario_data
	global sens_alt
	global gps_alt
	global sens_gps_mean
	global alt_diff
	global flight_duration
	# parse data
	file = sys.argv[1]
	parse_vario_data(file)
	# array to np.array
	vario_data = np.array(vario_data)
	# calculate mean value of sens and gps data for every entry
	for n in range(vario_data.shape[0]-1):
		sens_alt.append(vario_data[n][3])
		gps_alt.append(vario_data[n][4])
		sens_gps_mean.append(np.mean([sens_alt[-1], gps_alt[-1]]))
	sens_alt = np.array(sens_alt)
	gps_alt = np.array(gps_alt)
	sens_gps_mean = np.array(sens_gps_mean)
	# calculate diff of altitude in m/s
	alt_diff = np.array(np.diff(sens_gps_mean))
	# calculate flight duration
	takeoff_time = convert_time(vario_data[0][0])
	landing_time = convert_time(vario_data[vario_data.shape[0]-1][0])
	flight_duration = landing_time - takeoff_time

	# print
	print_flight_stats()


if __name__ == "__main__":
	main()
